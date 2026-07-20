use crate::clio;
use crate::cache::{CacheState, CachedList};
use crate::jobs::JobState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationInfo {
    pub id: String,
    pub name: String,
    pub code: String,
    pub version: String,
    pub description: Option<String>,
}

pub fn parse_applications_json(raw: &str) -> Result<Vec<ApplicationInfo>, String> {
    let value = serde_json::Deserializer::from_str(raw)
        .into_iter::<serde_json::Value>()
        .next()
        .ok_or_else(|| "clio returned no application JSON.".to_string())?
        .map_err(|error| format!("Invalid clio application JSON: {error}"))?;
    let rows = if let Some(array) = value.as_array() {
        array
    } else {
        value
            .get("data")
            .and_then(|data| data.as_array())
            .ok_or_else(|| "clio application JSON has no data array.".to_string())?
    };
    let read = |row: &serde_json::Value, upper: &str, lower: &str| {
        row.get(upper)
            .or_else(|| row.get(lower))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut applications = rows
        .iter()
        .filter_map(|row| {
            let code = read(row, "Code", "code");
            if code.is_empty() {
                return None;
            }
            Some(ApplicationInfo {
                id: read(row, "Id", "id"),
                name: read(row, "Name", "name"),
                code,
                version: read(row, "Version", "version"),
                description: row
                    .get("Description")
                    .or_else(|| row.get("description"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    applications.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(applications)
}

#[tauri::command]
pub fn list_applications(
    cache: State<'_, CacheState>,
    env: String,
    force_refresh: bool,
) -> Result<CachedList<ApplicationInfo>, String> {
    if !force_refresh {
        if let Some(cached) = cache.get("applications", &env) {
            return Ok(cached);
        }
    }
    let (code, output) = clio::clio_capture(&["list-apps", "-e", &env, "--json"])?;
    if code != 0 {
        let base = format!("clio list-apps failed for {env} (exit {code}): {}", output.trim());
        return Err(match clio::diagnose(&output) {
            Some(hint) => format!("{base} — {hint}"),
            None => base,
        });
    }
    let applications = parse_applications_json(&output)?;
    Ok(cache.put("applications", &env, &applications))
}

#[tauri::command]
pub fn deploy_application_between_environments(
    app: AppHandle,
    jobs: State<'_, JobState>,
    source_env: String,
    target_env: String,
    app_code: String,
) -> Result<String, String> {
    let app_code = app_code.trim().to_string();
    if app_code.is_empty() || app_code.starts_with('-') {
        return Err("Invalid application code.".to_string());
    }
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
        "deploy-application",
        Some(target_env.clone()),
        format!("deploy app {app_code}: {source_env} → {target_env}"),
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
        if !state.mark_running_phase(
            &app,
            &job_id,
            "transferring and installing application",
            false,
        ) {
            return;
        }
        state.log(
            &app,
            &job_id,
            format!(
                "Deploying application {app_code} from {source_env} to {target_env}. This clio operation downloads, transfers, and installs the application as one server-side workflow and cannot be safely cancelled after it starts."
            ),
        );
        let args = vec![
            "deploy-application".into(),
            app_code.clone(),
            "-e".into(),
            source_env.clone(),
            "-d".into(),
            target_env.clone(),
        ];
        match state.stream_process(&app, &job_id, "clio", &args, None, &[]) {
            Ok(0) => {
                state.log(
                    &app,
                    &job_id,
                    format!("✓ {app_code} deployed from {source_env} to {target_env}."),
                );
                state.finish(&app, &job_id, Some(0));
            }
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
    fn parses_application_array_and_trailing_warning() {
        let raw = r#"[
          {"Id":"1","Name":"Thoughtworks Core","Code":"TwkCore","Version":"1.0.2","Description":null},
          {"Id":"2","Name":"Thoughtworks Reporting","Code":"TwkReporting","Version":"1.0.0","Description":"Reports"}
        ]
        [WAR] - a newer clio is available"#;
        assert_eq!(
            parse_applications_json(raw).unwrap(),
            vec![
                ApplicationInfo {
                    id: "1".into(),
                    name: "Thoughtworks Core".into(),
                    code: "TwkCore".into(),
                    version: "1.0.2".into(),
                    description: None
                },
                ApplicationInfo {
                    id: "2".into(),
                    name: "Thoughtworks Reporting".into(),
                    code: "TwkReporting".into(),
                    version: "1.0.0".into(),
                    description: Some("Reports".into())
                }
            ]
        );
    }
}
