use crate::clio;
use crate::cache::{CacheState, CachedList};
use crate::jobs::JobState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub maintainer: String,
}

/// Whether a package is locked, keyed by name. Separate from `PackageInfo`
/// because `clio list-packages -j` does not report it at all — see
/// `package_lock_states`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLockState {
    pub name: String,
    pub locked: bool,
}

/// Lock state for every package in `env`, read from `SysPackage.InstallType`.
///
/// `clio lock-package` works by setting `InstallType = 1`, and `list-packages`
/// returns no lock field whatsoever, so the list can only show a lock badge —
/// or offer a meaningful toggle — with this second read. One query for the
/// whole list, following `applications::application_extras`: an environment
/// without cliogate gets an `Err` the Packages screen ignores, and the table
/// renders exactly as it did before.
// `(async)` runs these clio-backed reads off the UI thread — see sql::run_sql.
#[tauri::command(async)]
pub fn package_lock_states(env: String) -> Result<Vec<PackageLockState>, String> {
    let sql = r#"SELECT p."Name", p."InstallType" FROM "SysPackage" p ORDER BY p."Name""#;
    let result = crate::sql::query_env(&env, sql)?;
    let index = |name: &str| {
        result
            .columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(name))
    };
    let (Some(name_at), Some(type_at)) = (index("Name"), index("InstallType")) else {
        return Err("SysPackage did not return Name and InstallType.".to_string());
    };
    Ok(result
        .rows
        .iter()
        .filter_map(|row| {
            let name = row.get(name_at)?.trim();
            if name.is_empty() {
                return None;
            }
            Some(PackageLockState {
                name: name.to_string(),
                locked: row.get(type_at).map(|value| value.trim() == "1")?,
            })
        })
        .collect())
}

/// Parse the human-readable `clio list-packages` table. clio may prepend
/// [INF]/[WAR] messages and may render a separator row below the header.
#[cfg(test)]
pub fn parse_package_list(raw: &str) -> Vec<PackageInfo> {
    let mut packages = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('[')
            || trimmed.starts_with("Name ")
            || trimmed.chars().all(|c| c == '-' || c == '─' || c.is_whitespace())
        {
            continue;
        }

        let mut columns = trimmed.split_whitespace();
        let (Some(name), Some(version)) = (columns.next(), columns.next()) else {
            continue;
        };
        if !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        packages.push(PackageInfo {
            name: name.to_string(),
            version: version.to_string(),
            maintainer: columns.collect::<Vec<_>>().join(" "),
        });
    }
    packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packages
}

/// Parse the unified JSON envelope emitted by modern clio versions. A warning
/// may be printed after the JSON document, so deserialize only the first value.
pub fn parse_package_json(raw: &str) -> Result<Vec<PackageInfo>, String> {
    let value = serde_json::Deserializer::from_str(raw)
        .into_iter::<serde_json::Value>()
        .next()
        .ok_or_else(|| "clio returned no JSON.".to_string())?
        .map_err(|e| format!("Invalid clio package JSON: {e}"))?;
    if value.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        let message = value
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("clio could not list packages");
        return Err(message.to_string());
    }
    let rows = value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "clio package JSON has no data array.".to_string())?;
    let mut packages = rows
        .iter()
        .filter_map(|row| {
            let descriptor = row.get("Descriptor").or_else(|| row.get("descriptor"))?;
            let string = |upper: &str, lower: &str| {
                descriptor
                    .get(upper)
                    .or_else(|| descriptor.get(lower))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let name = string("Name", "name");
            if name.is_empty() {
                return None;
            }
            Some(PackageInfo {
                name,
                version: string("PackageVersion", "packageVersion"),
                maintainer: string("Maintainer", "maintainer"),
            })
        })
        .collect::<Vec<_>>();
    packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(packages)
}

#[tauri::command(async)]
pub fn list_packages(
    cache: State<'_, CacheState>,
    env: String,
    force_refresh: bool,
) -> Result<CachedList<PackageInfo>, String> {
    if !force_refresh {
        if let Some(cached) = cache.get("packages", &env) {
            return Ok(cached);
        }
    }
    let (code, output) = clio::clio_capture(&["list-packages", "-e", &env, "-j"])?;
    if code != 0 {
        let base = format!("clio list-packages failed for {env} (exit {code}): {}", output.trim());
        return Err(match clio::diagnose(&output) {
            Some(hint) => format!("{base} — {hint}"),
            None => base,
        });
    }
    let packages = parse_package_json(&output)?;
    Ok(cache.put("packages", &env, &packages))
}

fn require_package_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Package name is required.".to_string());
    }
    if name.contains(',') || name.starts_with('-') {
        return Err("Invalid package name.".to_string());
    }
    Ok(name.to_string())
}

fn require_existing_path(path: Option<String>) -> Result<PathBuf, String> {
    let raw = path.unwrap_or_default();
    let path = PathBuf::from(raw.trim());
    if raw.trim().is_empty() || !path.exists() {
        return Err("Choose an existing package folder or .zip/.gz archive.".to_string());
    }
    Ok(path)
}

fn run_job(
    app: AppHandle,
    jobs: State<'_, JobState>,
    env: String,
    kind: String,
    display: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
) -> String {
    let id = jobs.create_job(&app, &kind, Some(env.clone()), display);
    let lock = jobs.env_lock(Some(&env));
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        let (phase, cancellable) = if kind == "pull-package" {
            ("downloading package", true)
        } else {
            ("modifying environment", false)
        };
        if !state.mark_running_phase(&app, &job_id, phase, cancellable) {
            return;
        }
        match state.stream_process(&app, &job_id, "clio", &args, cwd.as_deref(), &[]) {
            Ok(code) => state.finish(&app, &job_id, Some(code)),
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    id
}

fn find_downloaded_package(root: &Path, package: &str) -> Option<PathBuf> {
    let direct = root.join(package);
    if direct.is_dir() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("descriptor.json").is_file() {
            return Some(path);
        }
    }
    None
}

fn run_version_job(
    app: AppHandle,
    jobs: State<'_, JobState>,
    env: String,
    package: String,
    version: String,
    skip_backup: bool,
) -> String {
    let id = jobs.create_job(
        &app,
        "set-package-version",
        Some(env.clone()),
        format!("set {package} version to {version} on {env}"),
    );
    let lock = jobs.env_lock(Some(&env));
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !state.mark_running_phase(&app, &job_id, "downloading package", true) {
            return;
        }
        let temp = std::env::temp_dir().join(format!(
            "creatio-devhub-version-{}-{}",
            std::process::id(),
            crate::jobs::now_ms()
        ));
        let result = (|| -> Result<i32, String> {
            std::fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
            state.log(
                &app,
                &job_id,
                "Version changes are applied safely as pull package → edit descriptor → push package.",
            );
            let pull = vec![
                "pull-pkg".into(),
                package.clone(),
                "-e".into(),
                env.clone(),
                "--unzip".into(),
            ];
            let code = state.stream_process(&app, &job_id, "clio", &pull, Some(&temp), &[])?;
            if code != 0 {
                return Ok(code);
            }
            if state.is_cancel_requested(&job_id) {
                return Ok(1);
            }
            if !state.set_phase(&app, &job_id, "updating local descriptor", true) {
                return Ok(1);
            }
            let package_path = find_downloaded_package(&temp, &package)
                .ok_or_else(|| "clio downloaded the package, but its folder could not be located.".to_string())?;
            let set_version = vec![
                "set-pkg-version".into(),
                package_path.to_string_lossy().to_string(),
                "-v".into(),
                version,
            ];
            let code = state.stream_process(&app, &job_id, "clio", &set_version, None, &[])?;
            if code != 0 {
                return Ok(code);
            }
            if state.is_cancel_requested(&job_id) {
                return Ok(1);
            }
            if !state.set_phase(
                &app,
                &job_id,
                "installing package in environment",
                false,
            ) {
                return Ok(1);
            }
            let mut push = vec![
                "push-pkg".into(),
                package_path.to_string_lossy().to_string(),
                "-e".into(),
                env,
            ];
            if skip_backup {
                push.extend(["--skip-backup".into(), "true".into()]);
            }
            state.stream_process(&app, &job_id, "clio", &push, None, &[])
        })();
        let _ = std::fs::remove_dir_all(&temp);
        match result {
            Ok(code) => state.finish(&app, &job_id, Some(code)),
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    id
}

/// Run a single-package operation using a strict action allow-list. Destructive
/// confirmation is owned by the UI; the backend still refuses malformed inputs.
#[tauri::command]
pub fn run_package_action(
    app: AppHandle,
    jobs: State<'_, JobState>,
    env: String,
    package: String,
    action: String,
    path: Option<String>,
    value: Option<String>,
    skip_backup: Option<bool>,
) -> Result<String, String> {
    let package = require_package_name(&package)?;
    if action == "version" {
        let version = value.unwrap_or_default();
        let version = version.trim();
        if version.is_empty()
            || !version.chars().all(|c| c.is_ascii_digit() || c == '.')
            || version.split('.').any(|part| part.is_empty())
        {
            return Err("Enter a numeric package version such as 1.2.3.4.".to_string());
        }
        return Ok(run_version_job(
            app,
            jobs,
            env,
            package,
            version.to_string(),
            skip_backup.unwrap_or(false),
        ));
    }
    let (kind, display, args, cwd) = match action.as_str() {
        "pull" => {
            let dir = require_existing_path(path)?;
            if !dir.is_dir() {
                return Err("Choose a destination folder for the downloaded package.".to_string());
            }
            (
                "pull-package".to_string(),
                format!("pull {package} ← {env}"),
                vec!["pull-pkg".into(), package.clone(), "-e".into(), env.clone()],
                Some(dir),
            )
        }
        "push" => {
            let source = require_existing_path(path)?;
            let mut args = vec![
                "push-pkg".into(),
                source.to_string_lossy().to_string(),
                "-e".into(),
                env.clone(),
            ];
            if skip_backup.unwrap_or(false) {
                args.extend(["--skip-backup".into(), "true".into()]);
            }
            (
                "install-package".to_string(),
                format!("install {} → {env}", source.display()),
                args,
                None,
            )
        }
        "lock" => (
            "lock-package".to_string(),
            format!("lock {package} on {env}"),
            vec!["lock-package".into(), package.clone(), "-e".into(), env.clone()],
            None,
        ),
        "unlock" => (
            "unlock-package".to_string(),
            format!("unlock {package} on {env}"),
            vec!["unlock-package".into(), package.clone(), "-e".into(), env.clone()],
            None,
        ),
        "activate" => (
            "activate-package".to_string(),
            format!("activate {package} on {env}"),
            vec!["activate-pkg".into(), package.clone(), "-e".into(), env.clone()],
            None,
        ),
        "deactivate" => (
            "deactivate-package".to_string(),
            format!("deactivate {package} on {env}"),
            vec!["deactivate-pkg".into(), package.clone(), "-e".into(), env.clone()],
            None,
        ),
        "hotfix" => {
            let enabled = match value.as_deref() {
                Some("true") => "true",
                Some("false") => "false",
                _ => return Err("Hotfix value must be true or false.".to_string()),
            };
            (
                "package-hotfix".to_string(),
                format!("set {package} hotfix={enabled} on {env}"),
                vec![
                    "pkg-hotfix".into(),
                    package.clone(),
                    enabled.into(),
                    "-e".into(),
                    env.clone(),
                ],
                None,
            )
        }
        "delete" => (
            "delete-package".to_string(),
            format!("delete {package} from {env}"),
            vec!["delete-pkg-remote".into(), package.clone(), "-e".into(), env.clone()],
            None,
        ),
        _ => return Err(format!("Unsupported package action: {action}")),
    };
    Ok(run_job(app, jobs, env, kind, display, args, cwd))
}

/// Transfer a package between registered environments using a temporary local
/// download. The source and target locks are acquired in sorted order so two
/// opposite deployments cannot deadlock.
#[tauri::command]
pub fn deploy_package_between_environments(
    app: AppHandle,
    jobs: State<'_, JobState>,
    source_env: String,
    target_env: String,
    package: String,
    skip_backup: bool,
) -> Result<String, String> {
    let package = require_package_name(&package)?;
    if source_env == target_env {
        return Err("Choose a different target environment.".to_string());
    }
    let environments = clio::list_environments()?;
    for name in [&source_env, &target_env] {
        if !environments.iter().any(|environment| &environment.name == name) {
            return Err(format!("Environment {name} is not registered in clio."));
        }
    }

    let id = jobs.create_job(
        &app,
        "deploy-package",
        Some(target_env.clone()),
        format!("deploy {package}: {source_env} → {target_env}"),
    );
    let (first_env, second_env) = if source_env < target_env {
        (source_env.clone(), target_env.clone())
    } else {
        (target_env.clone(), source_env.clone())
    };
    let first_lock = jobs.env_lock(Some(&first_env));
    let second_lock = jobs.env_lock(Some(&second_env));
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let _first_guard = first_lock.lock().unwrap();
        let _second_guard = second_lock.lock().unwrap();
        if !state.mark_running_phase(&app, &job_id, "downloading from source", true) {
            return;
        }
        let temp = std::env::temp_dir().join(format!(
            "creatio-devhub-deploy-{}-{}",
            std::process::id(),
            crate::jobs::now_ms()
        ));
        let result = (|| -> Result<i32, String> {
            std::fs::create_dir_all(&temp).map_err(|error| error.to_string())?;
            state.log(
                &app,
                &job_id,
                format!("Downloading {package} from {source_env}…"),
            );
            let pull = vec![
                "pull-pkg".into(),
                package.clone(),
                "-e".into(),
                source_env.clone(),
                "--unzip".into(),
            ];
            let code =
                state.stream_process(&app, &job_id, "clio", &pull, Some(&temp), &[])?;
            if code != 0 {
                return Ok(code);
            }
            if state.is_cancel_requested(&job_id) {
                return Ok(1);
            }
            let package_path = find_downloaded_package(&temp, &package).ok_or_else(|| {
                "clio downloaded the package, but its folder could not be located.".to_string()
            })?;
            state.log(
                &app,
                &job_id,
                format!("Installing {package} into {target_env}…"),
            );
            if !state.set_phase(
                &app,
                &job_id,
                "installing in target environment",
                false,
            ) {
                return Ok(1);
            }
            let mut push = vec![
                "push-pkg".into(),
                package_path.to_string_lossy().to_string(),
                "-e".into(),
                target_env.clone(),
            ];
            if skip_backup {
                push.extend(["--skip-backup".into(), "true".into()]);
            }
            let code = state.stream_process(&app, &job_id, "clio", &push, None, &[])?;
            if code == 0 {
                state.log(
                    &app,
                    &job_id,
                    format!("✓ {package} deployed from {source_env} to {target_env}."),
                );
            }
            Ok(code)
        })();
        let _ = std::fs::remove_dir_all(&temp);
        match result {
            Ok(code) => state.finish(&app, &job_id, Some(code)),
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_table_and_skips_clio_noise() {
        let raw = "\
[WAR] - clio 8.1.0.99 is available.
[INF] - Name                Version      Maintainer
──────────────────────────────────────────────────
Base                         8.3.0        Creatio
QntImportEngine              1.2.3.4      Qnovate Labs
CampaignElements.UI          7.8.0        Creatio
";
        assert_eq!(
            parse_package_list(raw),
            vec![
                PackageInfo {
                    name: "Base".into(),
                    version: "8.3.0".into(),
                    maintainer: "Creatio".into()
                },
                PackageInfo {
                    name: "CampaignElements.UI".into(),
                    version: "7.8.0".into(),
                    maintainer: "Creatio".into()
                },
                PackageInfo {
                    name: "QntImportEngine".into(),
                    version: "1.2.3.4".into(),
                    maintainer: "Qnovate Labs".into()
                },
            ]
        );
    }

    #[test]
    fn rejects_non_package_rows() {
        let raw = "No packages were found\nError Unauthorized\n";
        assert!(parse_package_list(raw).is_empty());
    }

    #[test]
    fn parses_json_packages_with_empty_versions_and_trailing_warning() {
        let raw = r#"{
          "schemaVersion": "1.0",
          "ok": true,
          "data": [
            {"Descriptor":{"Name":"NoVersion","PackageVersion":"","Maintainer":"Customer"}},
            {"Descriptor":{"Name":"Versioned","PackageVersion":"1.2.3","Maintainer":"Qnovate"}}
          ],
          "error": null
        }
        [WAR] - a newer clio is available"#;
        assert_eq!(
            parse_package_json(raw).unwrap(),
            vec![
                PackageInfo {
                    name: "NoVersion".into(),
                    version: "".into(),
                    maintainer: "Customer".into()
                },
                PackageInfo {
                    name: "Versioned".into(),
                    version: "1.2.3".into(),
                    maintainer: "Qnovate".into()
                }
            ]
        );
    }
}
