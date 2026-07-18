use serde::Serialize;
use std::path::PathBuf;

/// clio's own settings file — the single source of truth for environments.
/// We read it only to LIST environments; secrets are never exposed to the UI.
pub fn settings_path() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA not set".to_string())?;
    Ok(PathBuf::from(base).join("creatio").join("clio").join("appsettings.json"))
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

#[tauri::command]
pub fn set_default_environment(name: String) -> Result<(), String> {
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
    Ok(())
}

/// Run clio synchronously and capture output (for quick reads like list-packages).
pub fn clio_capture(args: &[&str]) -> Result<(i32, String), String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("clio");
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| format!("Failed to start clio: {e}"))?;
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

    /// Runs against the machine's real clio settings file when present.
    #[test]
    fn parses_local_clio_settings() {
        let path = settings_path().expect("LOCALAPPDATA should be set");
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
