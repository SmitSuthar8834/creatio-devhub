//! Locating the external CLIs DevHub drives (clio, git, gh, dotnet).
//!
//! `Command::new("gh")` resolves only against the PATH this process inherited.
//! On Windows that is Explorer's snapshot from login time, so a tool installed
//! after the user last signed in stays invisible to the app while working fine
//! in any freshly-opened terminal. CreateProcess also only appends `.exe`, so
//! `gh.cmd`-style shims (scoop, npm wrappers) never resolve either. Both cases
//! surface as "not installed", which is wrong and unactionable.
//!
//! So we do the lookup ourselves, in order: user override → inherited PATH →
//! the *live* PATH read back from the registry → the tool's well-known install
//! locations. The result is cached until `clear_cache` (the Refresh buttons).

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

/// The CLIs we know how to hunt for. Anything else resolves via PATH only.
pub const KNOWN_TOOLS: [&str; 4] = ["clio", "git", "gh", "dotnet"];

fn cache() -> &'static Mutex<HashMap<String, Option<PathBuf>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn overrides() -> &'static Mutex<HashMap<String, String>> {
    static OVERRIDES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    OVERRIDES.get_or_init(Default::default)
}

fn override_file() -> &'static Mutex<Option<PathBuf>> {
    static FILE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    FILE.get_or_init(Default::default)
}

/// Load the saved per-tool path overrides. Called once at startup.
pub fn init(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("tool-paths.json");
    let saved: HashMap<String, String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    if let Ok(mut map) = overrides().lock() {
        *map = saved;
    }
    if let Ok(mut slot) = override_file().lock() {
        *slot = Some(file);
    }
}

fn save_overrides() {
    let (Ok(map), Ok(slot)) = (overrides().lock(), override_file().lock()) else {
        return;
    };
    let Some(file) = slot.as_ref() else { return };
    if let Ok(json) = serde_json::to_string_pretty(&*map) {
        let _ = std::fs::write(file, json);
    }
}

/// The path to run `program` with: an absolute path when we can find one,
/// otherwise the bare name so the OS gets its normal chance (and any failure
/// still reports the name the user recognizes).
pub fn resolve(program: &str) -> String {
    locate(program)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| program.to_string())
}

/// Find `program` on disk, caching both hits and misses.
pub fn locate(program: &str) -> Option<PathBuf> {
    if let Ok(cache) = cache().lock() {
        if let Some(hit) = cache.get(program) {
            return hit.clone();
        }
    }
    let found = search(program);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(program.to_string(), found.clone());
    }
    found
}

/// Drop memoized lookups so a tool installed while DevHub is open is picked up
/// without a restart.
pub fn clear_cache() {
    if let Ok(mut cache) = cache().lock() {
        cache.clear();
    }
}

fn search(program: &str) -> Option<PathBuf> {
    if let Ok(map) = overrides().lock() {
        if let Some(custom) = map.get(program) {
            let path = PathBuf::from(custom);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    for dir in search_dirs() {
        if let Some(hit) = probe_dir(&dir, program) {
            return Some(hit);
        }
    }
    well_known(program).into_iter().find(|path| path.is_file())
}

/// PATH as inherited, then PATH as it exists *now* in the registry — the second
/// is what makes a post-login install visible without signing out.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    for dir in live_path_dirs() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

#[cfg(not(windows))]
fn live_path_dirs() -> Vec<PathBuf> {
    Vec::new()
}

/// Read HKCU/HKLM `Path` via reg.exe (absolute path, so this can't recurse into
/// our own resolution) and expand any `%VAR%` references it contains.
#[cfg(windows)]
fn live_path_dirs() -> Vec<PathBuf> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let reg = format!("{root}\\System32\\reg.exe");
    let keys = [
        "HKCU\\Environment",
        "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
    ];
    let mut dirs = Vec::new();
    for key in keys {
        if let Some(out) = reg_query(&reg, key) {
            dirs.extend(parse_reg_path(&out));
        }
    }
    dirs
}

/// Pull the directories out of `reg query ... /v Path` output, which looks like
/// "    Path    REG_EXPAND_SZ    C:\a;%USERPROFILE%\b".
#[cfg(windows)]
fn parse_reg_path(out: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        if !parts.next().is_some_and(|name| name.eq_ignore_ascii_case("Path")) {
            continue;
        }
        let Some(kind) = parts.next() else { continue };
        let Some(start) = line.find(kind).map(|at| at + kind.len()) else {
            continue;
        };
        for entry in expand_env(line[start..].trim()).split(';') {
            let entry = entry.trim();
            if !entry.is_empty() {
                dirs.push(PathBuf::from(entry));
            }
        }
    }
    dirs
}

#[cfg(windows)]
fn reg_query(reg: &str, key: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Command::new(reg)
        .args(["query", key, "/v", "Path"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Substitute `%VAR%` from the current environment; unknown names are left as-is.
#[cfg(windows)]
fn expand_env(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('%') {
            Some(close) => {
                let name = &after[..close];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push('%');
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Candidate file names for `program` in a directory: bare on Unix, and every
/// PATHEXT variant on Windows so `.cmd`/`.bat` shims are found too.
fn candidates(program: &str) -> Vec<String> {
    if !cfg!(windows) || Path::new(program).extension().is_some() {
        return vec![program.to_string()];
    }
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut names: Vec<String> = exts
        .split(';')
        .map(str::trim)
        .filter(|ext| !ext.is_empty())
        .map(|ext| format!("{program}{}", ext.to_lowercase()))
        .collect();
    names.push(program.to_string());
    names
}

fn probe_dir(dir: &Path, program: &str) -> Option<PathBuf> {
    candidates(program)
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Default install locations, for when the tool is on nobody's PATH.
fn well_known(program: &str) -> Vec<PathBuf> {
    let env = |key: &str| std::env::var(key).ok().map(PathBuf::from);
    let home = env("USERPROFILE").or_else(|| env("HOME"));
    let program_files = env("ProgramFiles");
    let program_files_x86 = env("ProgramFiles(x86)");
    let local = env("LOCALAPPDATA");
    let program_data = env("ProgramData");

    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |base: Option<PathBuf>, tail: &str| {
        if let Some(base) = base {
            dirs.push(base.join(tail));
        }
    };

    match program {
        "gh" => {
            push(program_files.clone(), "GitHub CLI");
            push(program_files_x86.clone(), "GitHub CLI");
            push(local.clone(), "Programs\\GitHub CLI");
        }
        "git" => {
            push(program_files.clone(), "Git\\cmd");
            push(program_files_x86.clone(), "Git\\cmd");
            push(local.clone(), "Programs\\Git\\cmd");
        }
        "clio" => {
            push(home.clone(), ".dotnet\\tools");
        }
        "dotnet" => {
            push(program_files.clone(), "dotnet");
            push(local.clone(), "Microsoft\\dotnet");
        }
        _ => {}
    }
    // Package managers shim everything into one bin directory.
    push(local.clone(), "Microsoft\\WinGet\\Links");
    push(program_data.clone(), "chocolatey\\bin");
    push(home.clone(), "scoop\\shims");
    for unix in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(unix));
    }

    dirs.iter().flat_map(|dir| candidates(program).into_iter().map(|n| dir.join(n))).collect()
}

/// Human-readable "we looked here" list for error messages.
pub fn searched_locations(program: &str) -> Vec<String> {
    let mut seen: Vec<String> = vec!["PATH".to_string()];
    for path in well_known(program) {
        if let Some(dir) = path.parent() {
            let dir = dir.to_string_lossy().to_string();
            if !seen.contains(&dir) {
                seen.push(dir);
            }
        }
    }
    seen
}

/// The message shown when a tool genuinely can't be found anywhere.
pub fn not_found(program: &str) -> String {
    format!(
        "Could not find {program}. Searched: {}. If it is installed somewhere else, set its path in Settings.",
        searched_locations(program).join(", ")
    )
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPath {
    pub program: String,
    /// Where we resolved it, if at all.
    pub path: Option<String>,
    /// The user-configured override, if one is set.
    pub custom: Option<String>,
    pub searched: Vec<String>,
}

fn describe(program: &str) -> ToolPath {
    ToolPath {
        program: program.to_string(),
        path: locate(program).map(|path| path.to_string_lossy().to_string()),
        custom: overrides().lock().ok().and_then(|map| map.get(program).cloned()),
        searched: searched_locations(program),
    }
}

/// Where DevHub resolved each CLI — shown in Settings for diagnosis.
#[tauri::command]
pub fn tool_paths() -> Vec<ToolPath> {
    clear_cache();
    KNOWN_TOOLS.iter().map(|program| describe(program)).collect()
}

/// Pin `program` to an explicit executable, or clear the pin with an empty path.
#[tauri::command]
pub fn set_tool_path(program: String, path: String) -> Result<ToolPath, String> {
    let trimmed = path.trim().to_string();
    if !trimmed.is_empty() && !Path::new(&trimmed).is_file() {
        return Err(format!("No file at {trimmed}."));
    }
    if let Ok(mut map) = overrides().lock() {
        if trimmed.is_empty() {
            map.remove(&program);
        } else {
            map.insert(program.clone(), trimmed);
        }
    }
    save_overrides();
    clear_cache();
    Ok(describe(&program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_cover_shims_on_windows() {
        let names = candidates("gh");
        assert!(names.contains(&"gh".to_string()));
        if cfg!(windows) {
            assert!(names.contains(&"gh.cmd".to_string()));
            assert!(names.contains(&"gh.exe".to_string()));
        }
    }

    #[test]
    fn candidates_respect_an_explicit_extension() {
        assert_eq!(candidates("gh.exe"), vec!["gh.exe".to_string()]);
    }

    #[test]
    fn unknown_tools_still_fall_back_to_the_bare_name() {
        assert_eq!(resolve("definitely-not-a-real-tool"), "definitely-not-a-real-tool");
    }

    #[test]
    fn searched_locations_start_with_path() {
        let searched = searched_locations("gh");
        assert_eq!(searched[0], "PATH");
        assert!(searched.len() > 1);
    }

    #[cfg(windows)]
    #[test]
    fn parses_the_path_value_out_of_reg_output() {
        std::env::set_var("DEVHUB_TEST_HOME", "C:\\Users\\test");
        let out = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Temp    REG_SZ    C:\\tmp\r\n    Path    REG_EXPAND_SZ    C:\\Program Files\\GitHub CLI\\;%DEVHUB_TEST_HOME%\\.dotnet\\tools;\r\n";
        let dirs = parse_reg_path(out);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("C:\\Program Files\\GitHub CLI\\"),
                PathBuf::from("C:\\Users\\test\\.dotnet\\tools"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn expand_env_substitutes_known_vars_only() {
        std::env::set_var("DEVHUB_TEST_VAR", "C:\\tools");
        assert_eq!(expand_env("%DEVHUB_TEST_VAR%\\bin"), "C:\\tools\\bin");
        assert_eq!(expand_env("%DEVHUB_NOT_SET%\\bin"), "%DEVHUB_NOT_SET%\\bin");
        assert_eq!(expand_env("C:\\plain"), "C:\\plain");
    }
}
