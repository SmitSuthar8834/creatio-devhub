use crate::jobs::JobState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

/// clio's own settings file — the single source of truth for environments.
/// We read it only to LIST environments; secrets are never exposed to the UI.
pub fn settings_path() -> Result<PathBuf, String> {
    // clio stores appsettings.json at a per-platform base; the subpath
    // creatio/clio/appsettings.json is the same everywhere. Verified against a
    // real install via `clio ver` ("settings file path: ..."):
    //   Windows:     %LOCALAPPDATA%\creatio\clio\appsettings.json
    //   macOS/Linux: $HOME/creatio/clio/appsettings.json
    // (Not the .NET SpecialFolder.LocalApplicationData `~/.local/share` location
    // one might expect — clio uses the home dir directly on Unix.)
    let rel = |base: PathBuf| base.join("creatio").join("clio").join("appsettings.json");

    if cfg!(windows) {
        let base = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA not set".to_string())?;
        return Ok(rel(PathBuf::from(base)));
    }

    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let primary = rel(PathBuf::from(&home));
    if primary.exists() {
        return Ok(primary);
    }
    // Fallback for any clio build that follows .NET's LocalApplicationData
    // instead: $XDG_DATA_HOME, else ~/.local/share.
    let xdg = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{home}/.local/share"));
    let alt = rel(PathBuf::from(xdg));
    if alt.exists() {
        return Ok(alt);
    }
    // Neither present yet — return the canonical home-dir location.
    Ok(primary)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSummary {
    pub name: String,
    pub uri: String,
    pub auth_kind: String, // "oauth" | "password" | "none"
    pub is_net_core: bool,
    pub is_active: bool,
    pub developer_mode: bool,
    pub maintainer: String,
}

#[tauri::command]
pub fn list_environments() -> Result<Vec<EnvSummary>, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let active = json
        .get("ActiveEnvironmentKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut envs = Vec::new();
    if let Some(map) = json.get("Environments").and_then(|v| v.as_object()) {
        for (name, e) in map {
            let str_of = |key: &str| e.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let bool_of = |key: &str| e.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            let auth_kind = if !str_of("ClientId").is_empty() && !str_of("ClientSecret").is_empty() {
                "oauth"
            } else if !str_of("Login").is_empty() {
                "password"
            } else {
                "none"
            };
            envs.push(EnvSummary {
                name: name.clone(),
                uri: str_of("Uri"),
                auth_kind: auth_kind.to_string(),
                is_net_core: bool_of("IsNetCore"),
                is_active: *name == active,
                developer_mode: bool_of("DeveloperModeEnabled"),
                maintainer: str_of("Maintainer"),
            });
        }
    }
    envs.sort_by(|a, b| (!a.is_active).cmp(&!b.is_active).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(envs)
}

// `(async)` — this shells `clio reg-web-app` and then re-reads the settings;
// off the UI thread so switching the default env can't freeze the window.
#[tauri::command(async)]
pub fn set_default_environment(app: AppHandle, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Choose an environment.".to_string());
    }
    let environments = list_environments()?;
    if !environments.iter().any(|environment| environment.name == name) {
        return Err(format!("Environment {name} is not registered in clio."));
    }
    let (code, output) = clio_capture(&["reg-web-app", "-a", name])?;
    if code != 0 {
        return Err(format!(
            "Could not set the default environment (exit {code}): {}",
            output.trim()
        ));
    }
    let active = list_environments()?
        .into_iter()
        .find(|environment| environment.is_active)
        .map(|environment| environment.name);
    if active.as_deref() != Some(name) {
        return Err("clio completed without updating the active environment.".to_string());
    }
    // Let the shell warm this environment's catalog cache in the background.
    let _ = app.emit("environment-changed", name.to_string());
    Ok(())
}

/// Whether the clio CLI itself is installed, and whether a newer build exists.
/// `clio ver` reports the installed version, the cliogate version and dotnet, and
/// prints a "clio X is available" warning when an update is published.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClioStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
    pub gate_version: Option<String>,
    pub dotnet: Option<String>,
    /// clio runs but its install is damaged (e.g. a missing assembly) — needs a repair.
    pub broken: bool,
}

/// Turn well-known clio / dotnet-tool failures into a single line of guidance.
/// Returns None when we don't recognize the failure (caller shows the raw output).
/// The rules live in `diagnostics` so the job log and this banner agree.
pub fn diagnose(output: &str) -> Option<String> {
    crate::diagnostics::diagnose(output).map(|found| {
        let steps = found
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| format!("{}. {step}", index + 1))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {} {steps}", found.summary, found.cause)
    })
}

/// Value after `key:` on the first line containing it, when it looks like a version.
fn parse_labeled_version(out: &str, key: &str) -> Option<String> {
    out.lines()
        .find(|line| line.contains(key))
        .and_then(|line| line.split(key).nth(1))
        .map(|value| value.trim().to_string())
        .filter(|value| value.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// The version mentioned in clio's "clio X.Y.Z is available" update notice.
fn parse_available_version(out: &str) -> Option<String> {
    out.lines()
        .find(|line| line.contains("is available"))
        .and_then(|line| line.split("is available").next())
        .and_then(|before| before.split_whitespace().last())
        .map(str::to_string)
        .filter(|value| value.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

// `(async)` — shells `clio ver` (and dotnet); keep it off the UI thread.
#[tauri::command(async)]
pub fn clio_status() -> ClioStatus {
    let dotnet = std::process::Command::new(crate::tools::resolve("dotnet"))
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty());

    match clio_capture(&["ver"]) {
        Ok((_, out)) => {
            let version = parse_labeled_version(&out, "clio:");
            let latest = parse_available_version(&out);
            let update_available = match (&version, &latest) {
                (Some(current), Some(newest)) => current != newest,
                _ => false,
            };
            // clio started but can't load its own dependencies → damaged install.
            let broken = out.to_lowercase().contains("could not load file or assembly");
            ClioStatus {
                installed: version.is_some() || broken,
                version,
                latest,
                update_available,
                gate_version: parse_labeled_version(&out, "gate:"),
                dotnet,
                broken,
            }
        }
        // clio_capture only errors when the executable can't be started at all.
        Err(_) => ClioStatus {
            installed: false,
            version: None,
            latest: None,
            update_available: false,
            gate_version: None,
            dotnet,
            broken: false,
        },
    }
}

/// Install, update, or repair the clio CLI via the .NET global-tool installer.
/// `mode` is "install" | "update" | "repair" — repair uninstalls first, which is
/// what fixes a damaged install (e.g. a missing assembly).
/// Output is captured rather than streamed so we can diagnose known failures
/// (locked files, missing SDK) instead of showing a misleading generic message.
#[tauri::command]
pub fn install_or_update_clio(
    app: AppHandle,
    jobs: State<'_, JobState>,
    mode: String,
) -> Result<String, String> {
    let mode = mode.trim().to_lowercase();
    let action = match mode.as_str() {
        "install" | "repair" => "install",
        "update" => "update",
        other => return Err(format!("Unknown clio action: {other}")),
    };
    let repair = mode == "repair";
    let id = jobs.create_job(
        &app,
        match mode.as_str() {
            "update" => "update-clio",
            "repair" => "repair-clio",
            _ => "install-clio",
        },
        None,
        format!("dotnet tool {} clio -g", if repair { "reinstall" } else { action }),
    );
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let phase = if repair { "repairing clio".to_string() } else { format!("{action}ing clio") };
        if !state.mark_running_phase(&app, &job_id, &phase, false) {
            return;
        }

        let log_output = |out: &str| {
            for line in out.lines().filter(|l| !l.trim().is_empty()) {
                state.log(&app, &job_id, line.to_string());
            }
        };

        if repair {
            state.log(&app, &job_id, "$ dotnet tool uninstall clio -g");
            match capture_cmd("dotnet", &["tool", "uninstall", "clio", "-g"]) {
                Ok((_, out)) => log_output(&out), // a missing tool here is fine
                Err(e) => state.log(&app, &job_id, e),
            }
        }

        state.log(&app, &job_id, format!("$ dotnet tool {action} clio -g"));
        match capture_cmd("dotnet", &["tool", action, "clio", "-g"]) {
            Ok((0, out)) => {
                log_output(&out);
                state.log(&app, &job_id, "✓ clio is ready.");
                state.finish(&app, &job_id, Some(0));
            }
            Ok((code, out)) => {
                log_output(&out);
                match diagnose(&out) {
                    Some(hint) => state.log(&app, &job_id, format!("✗ {hint}")),
                    None => state.log(&app, &job_id, "✗ Failed — see the output above."),
                }
                state.finish(&app, &job_id, Some(code));
            }
            Err(e) => {
                state.log(&app, &job_id, format!("✗ {e}"));
                state.log(&app, &job_id, "The .NET SDK (dotnet) is required to manage clio.");
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    Ok(id)
}

/// Run clio synchronously and capture output (for quick reads like list-packages).
pub fn clio_capture(args: &[&str]) -> Result<(i32, String), String> {
    capture_cmd("clio", args)
}

/// Run any command synchronously, returning (exit code, stdout+stderr).
pub fn capture_cmd(program: &str, args: &[&str]) -> Result<(i32, String), String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(crate::tools::resolve(program));
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("Failed to start {program}: {e}. {}", crate::tools::not_found(program)))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code().unwrap_or(-1), text))
}

/// Normalize `clio packages` output into a sorted "name|version" snapshot used
/// for drift detection. Log-prefix lines and the header are dropped, so an
/// update banner appearing later doesn't register as drift.
pub fn parse_packages_snapshot(raw: &str) -> Vec<String> {
    let mut rows = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name == "Name" && version == "Version" {
            continue;
        }
        if !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        rows.push(format!("{name}|{version}"));
    }
    rows.sort();
    rows
}

/// Current package snapshot of an environment, or Err when it can't be read.
pub fn packages_snapshot(env: &str) -> Result<String, String> {
    let (code, out) = clio_capture(&["packages", "-e", env])?;
    let rows = parse_packages_snapshot(&out);
    if rows.is_empty() {
        return Err(format!("Could not read package list from {env} (exit {code})."));
    }
    Ok(rows.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_snapshot_parses_table_and_skips_noise() {
        let raw = "\
[WAR] - clio 8.1.0.86 is available. Run 'dotnet tool update clio -g' to update.

[INF] - Name                Version      Maintainer

ActionsDashboard    7.8.0        Creatio
Base                7.8.0        Creatio
CampaignElements.UI 7.8.0        Creatio
";
        let rows = parse_packages_snapshot(raw);
        assert_eq!(rows, vec!["ActionsDashboard|7.8.0", "Base|7.8.0", "CampaignElements.UI|7.8.0"]);

        // A new update banner must not register as drift.
        let with_banner = format!("[WAR] - clio 9.0.0.1 is available.\n{raw}");
        assert_eq!(parse_packages_snapshot(&with_banner), rows);
    }

    #[test]
    fn diagnoses_real_clio_failures() {
        // Reported: `dotnet tool update clio -g` blocked by a locked tool store.
        let locked = "Tool 'clio' failed to update due to the following:\nFailed to uninstall tool package 'clio': Access to the path 'C:\\Users\\x\\.dotnet\\tools\\.store\\clio\\8.1.0.84' is denied.";
        assert!(diagnose(locked).expect("locked hint").contains("still running"));

        // Reported: clio list-apps failing on a damaged install.
        let broken = "[ERR] - Could not load file or assembly 'Creatio.Metrics.Abstractions, Version=1.0.5.0'. The system cannot find the file specified.";
        assert!(diagnose(broken).expect("broken hint").contains("Repair clio"));

        // Unknown failures fall through so the raw output is shown instead.
        assert_eq!(diagnose("some unexpected failure"), None);
    }

    #[test]
    fn parses_clio_version_and_update_notice() {
        // Real `clio ver` output shape.
        let out = "\
[WAR] - clio 8.1.0.86 is available. Run 'dotnet tool update clio -g' to update.
[INF] - clio:   8.1.0.84
[INF] - gate:   2.0.0.44
[INF] - dotnet:   10.0.10
[INF] - settings file path: C:\\Users\\x\\AppData\\Local\\creatio\\clio\\appsettings.json
";
        assert_eq!(parse_labeled_version(out, "clio:").as_deref(), Some("8.1.0.84"));
        assert_eq!(parse_labeled_version(out, "gate:").as_deref(), Some("2.0.0.44"));
        assert_eq!(parse_available_version(out).as_deref(), Some("8.1.0.86"));

        // No update notice → nothing reported as available.
        let current = "[INF] - clio:   8.1.0.86\n[INF] - gate:   2.0.0.44\n";
        assert_eq!(parse_available_version(current), None);
        // The settings-path line must not be mistaken for a version.
        assert_eq!(parse_labeled_version("[INF] - settings file path: C:\\x\\clio\\a.json", "clio:"), None);
    }

    /// Runs against the machine's real clio settings file when present.
    #[test]
    fn parses_local_clio_settings() {
        let Ok(path) = settings_path() else {
            eprintln!("no clio settings path on this platform — skipping");
            return;
        };
        if !path.exists() {
            eprintln!("clio settings not found — skipping");
            return;
        }
        let envs = list_environments().expect("settings file should parse");
        assert!(!envs.is_empty(), "expected at least one registered environment");
        for e in &envs {
            assert!(!e.name.is_empty());
            assert!(!e.uri.is_empty(), "environment {} has no Uri", e.name);
        }
        assert!(envs.iter().filter(|e| e.is_active).count() <= 1, "at most one active env");
    }
}
