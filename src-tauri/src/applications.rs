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

/// Descriptor facts clio's `list-apps` does not return. They live in
/// `SysInstalledApp`, so reading them needs SQL (and therefore cliogate) — every
/// caller treats this as an enhancement and still works without it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationExtras {
    pub code: String,
    pub maintainer: String,
    pub created_on: String,
    pub modified_on: String,
    pub required_platform_version: String,
    pub package_count: usize,
}

/// One package belonging to an application.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationPackage {
    pub name: String,
    pub version: String,
    pub maintainer: String,
}

/// A page (client schema) the application contributes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationPage {
    pub schema_name: String,
    pub package_name: String,
    pub parent_schema_name: String,
}

/// Everything the drill-down shows, assembled from two sources that fail
/// independently: `clio get-app-info --json` (pages, schema prefix) and SQL
/// (descriptor row, package list). Whichever answers is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDetails {
    pub code: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub maintainer: String,
    pub created_on: String,
    pub modified_on: String,
    pub install_date: String,
    pub last_update: String,
    pub required_platform_version: String,
    pub marketplace_link: String,
    pub help_link: String,
    pub support_email: String,
    pub is_hidden: String,
    pub needs_update: String,
    pub schema_name_prefix: String,
    pub packages: Vec<ApplicationPackage>,
    pub pages: Vec<ApplicationPage>,
    /// Why part of the picture is missing, when something failed. Shown as a
    /// note rather than an error, because the rest of the dialog is still real.
    pub notes: Vec<String>,
}

/// Read a column out of a SQL result row by header name. Missing columns yield
/// an empty string: a Creatio version without one should blank the field, not
/// fail the whole dialog.
fn column<'a>(columns: &[String], row: &'a [String], name: &str) -> &'a str {
    columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case(name))
        .and_then(|index| row.get(index))
        .map(String::as_str)
        .unwrap_or("")
}

/// SQL-escape a value for a single-quoted literal.
fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
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

/// Descriptor facts for every application in `env`, keyed by code.
///
/// One query for the whole list rather than one per tile. Returns an error only
/// when SQL itself is unavailable; the Applications screen ignores that and
/// shows the plain clio data.
#[tauri::command]
pub fn application_extras(env: String) -> Result<Vec<ApplicationExtras>, String> {
    let sql = r#"SELECT a."Code", a."Maintainer", a."CreatedOn", a."ModifiedOn",
       a."RequiredPlatformVersion", COUNT(pa."SysPackageId") AS "PackageCount"
FROM "SysInstalledApp" a
LEFT JOIN "SysPackageInInstalledApp" pa ON pa."SysInstalledAppId" = a."Id"
GROUP BY a."Code", a."Maintainer", a."CreatedOn", a."ModifiedOn", a."RequiredPlatformVersion""#;
    let result = crate::sql::query_env(&env, sql)?;
    Ok(result
        .rows
        .iter()
        .map(|row| ApplicationExtras {
            code: column(&result.columns, row, "Code").to_string(),
            maintainer: column(&result.columns, row, "Maintainer").to_string(),
            created_on: column(&result.columns, row, "CreatedOn").to_string(),
            modified_on: column(&result.columns, row, "ModifiedOn").to_string(),
            required_platform_version: column(&result.columns, row, "RequiredPlatformVersion")
                .to_string(),
            package_count: column(&result.columns, row, "PackageCount").parse().unwrap_or(0),
        })
        .filter(|extras| !extras.code.is_empty())
        .collect())
}

/// Everything known about one application.
///
/// Assembled from `clio get-app-info --json` and two SQL reads. Each source is
/// optional: a missing one adds a note and leaves its fields blank, so an
/// environment without cliogate still gets the pages and package name that clio
/// reports on its own.
#[tauri::command]
pub fn application_details(env: String, code: String) -> Result<ApplicationDetails, String> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err("No application code was supplied.".to_string());
    }
    let mut details = ApplicationDetails { code: code.clone(), ..Default::default() };

    match clio::clio_capture(&["get-app-info", "-e", &env, "--code", &code, "--json"]) {
        Ok((0, output)) => merge_app_info(&mut details, &output),
        Ok((_, output)) => details
            .notes
            .push(format!("clio could not read the application descriptor: {}", output.trim())),
        Err(error) => details.notes.push(error),
    }

    let literal = sql_literal(&code);
    let descriptor_sql = format!(
        r#"SELECT a."Name", a."Code", a."Version", a."Description", a."Maintainer",
       a."CreatedOn", a."ModifiedOn", a."InstallDate", a."LastUpdate",
       a."RequiredPlatformVersion", a."MarketplaceLink", a."HelpLink", a."SupportEmail",
       a."IsHidden", a."NeedUpdate"
FROM "SysInstalledApp" a WHERE a."Code" = '{literal}'"#
    );
    match crate::sql::query_env(&env, &descriptor_sql) {
        Ok(result) => {
            if let Some(row) = result.rows.first() {
                let field = |name: &str| column(&result.columns, row, name).to_string();
                if details.name.is_empty() {
                    details.name = field("Name");
                }
                if details.version.is_empty() {
                    details.version = field("Version");
                }
                details.description = field("Description");
                details.maintainer = field("Maintainer");
                details.created_on = field("CreatedOn");
                details.modified_on = field("ModifiedOn");
                details.install_date = field("InstallDate");
                details.last_update = field("LastUpdate");
                details.required_platform_version = field("RequiredPlatformVersion");
                details.marketplace_link = field("MarketplaceLink");
                details.help_link = field("HelpLink");
                details.support_email = field("SupportEmail");
                details.is_hidden = field("IsHidden");
                details.needs_update = field("NeedUpdate");
            }
        }
        Err(error) => details.notes.push(format!(
            "Descriptor details need SQL access to this environment (cliogate): {error}"
        )),
    }

    let packages_sql = format!(
        r#"SELECT p."Name", p."Version", p."Maintainer"
FROM "SysPackage" p
JOIN "SysPackageInInstalledApp" pa ON pa."SysPackageId" = p."Id"
JOIN "SysInstalledApp" a ON a."Id" = pa."SysInstalledAppId"
WHERE a."Code" = '{literal}'
ORDER BY p."Name""#
    );
    if let Ok(result) = crate::sql::query_env(&env, &packages_sql) {
        details.packages = result
            .rows
            .iter()
            .map(|row| ApplicationPackage {
                name: column(&result.columns, row, "Name").to_string(),
                version: column(&result.columns, row, "Version").to_string(),
                maintainer: column(&result.columns, row, "Maintainer").to_string(),
            })
            .filter(|package| !package.name.is_empty())
            .collect();
    }

    Ok(details)
}

/// Fold `clio get-app-info --json` into `details`.
///
/// clio prefixes the document with `[INF] - `, so parsing starts at the first
/// `{`. A malformed answer is a note, never a failure — SQL may still have
/// filled the dialog.
fn merge_app_info(details: &mut ApplicationDetails, output: &str) {
    let Some(start) = output.find('{') else {
        details.notes.push("clio returned no application descriptor.".to_string());
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&output[start..]) else {
        details.notes.push("clio's application descriptor could not be read.".to_string());
        return;
    };
    let text = |key: &str| {
        value.get(key).and_then(|value| value.as_str()).unwrap_or("").to_string()
    };
    details.name = text("ApplicationName");
    details.version = text("ApplicationVersion");
    details.schema_name_prefix = text("SchemaNamePrefix");
    details.pages = value
        .get("Pages")
        .and_then(|pages| pages.as_array())
        .map(|pages| {
            pages
                .iter()
                .map(|page| ApplicationPage {
                    schema_name: page
                        .get("schema-name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    package_name: page
                        .get("packageName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    parent_schema_name: page
                        .get("parentSchemaName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .filter(|page| !page.schema_name.is_empty())
                .collect()
        })
        .unwrap_or_default();
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

    /// Real `clio get-app-info --code QntCreatioERD --json` output, [INF] prefix
    /// and all.
    const APP_INFO: &str = r#"[WAR] - clio 8.1.0.87 is available. Run 'dotnet tool update clio -g' to update.
[INF] - {
  "PackageUId": "2ffe3cd2-6221-4d3b-948c-54fc3ed0dda5",
  "PackageName": "QntCreatioERD",
  "Entities": [],
  "Pages": [
    {"schema-name":"QntERDDiagramPage","uId":"8c51dae3","packageName":"QntCreatioERD","parentSchemaName":"PageWithAreaFreedomTemplate"},
    {"schema-name":"SystemDesigner","uId":"172b5440","packageName":"QntCreatioERD","parentSchemaName":"SystemDesigner"}
  ],
  "ApplicationId": "b5d1bbea-a974-449c-9fa4-33efdeb4f28e",
  "ApplicationName": "Creatio ERD",
  "ApplicationCode": "QntCreatioERD",
  "ApplicationVersion": "1.0.0",
  "SchemaNamePrefix": "Qnt"
}"#;

    #[test]
    fn reads_the_descriptor_clio_prints_after_its_log_prefix() {
        let mut details = ApplicationDetails::default();
        merge_app_info(&mut details, APP_INFO);
        assert_eq!(details.name, "Creatio ERD");
        assert_eq!(details.version, "1.0.0");
        assert_eq!(details.schema_name_prefix, "Qnt");
        assert_eq!(details.pages.len(), 2);
        assert_eq!(details.pages[1].schema_name, "SystemDesigner");
        assert_eq!(details.pages[0].parent_schema_name, "PageWithAreaFreedomTemplate");
        assert!(details.notes.is_empty());
    }

    #[test]
    fn unreadable_descriptor_becomes_a_note_not_a_failure() {
        let mut details = ApplicationDetails::default();
        merge_app_info(&mut details, "[INF] - not json at all");
        assert_eq!(details.notes.len(), 1);
        assert!(details.pages.is_empty());
    }

    #[test]
    fn columns_are_read_by_name_and_tolerate_absence() {
        let columns = vec!["Code".to_string(), "Maintainer".to_string()];
        let row = vec!["QntCreatioERD".to_string(), "Customer".to_string()];
        assert_eq!(column(&columns, &row, "Maintainer"), "Customer");
        // Case-insensitive, because clio echoes whatever the engine returned.
        assert_eq!(column(&columns, &row, "code"), "QntCreatioERD");
        // A column this Creatio version does not have blanks the field.
        assert_eq!(column(&columns, &row, "InstallDate"), "");
    }

    #[test]
    fn application_codes_cannot_break_out_of_the_sql_literal() {
        assert_eq!(sql_literal("O'Brien"), "O''Brien");
    }

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
