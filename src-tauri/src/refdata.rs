//! Lookup / reference-data comparison and migration between environments.
//!
//! Creatio's schema and configuration travel between environments with
//! `push-pkg` (see `packages.rs`), and any records *bound* to a package ride
//! along in its `Data/` folder. What neither carries is **unbound reference
//! data** — the values sitting in lookup tables that were added directly in one
//! environment. This module reads those, compares two environments' worth, and
//! (in a following step) migrates the differences dev → pre.
//!
//! **The model.** `SysLookup` is Creatio's registry of lookups; each row names a
//! lookup and points via `SysEntitySchemaUId` to the entity whose table holds
//! the actual values. A standard lookup entity inherits `BaseLookup`, giving it
//! `Id`, `Name` and `Description`. So enumeration is one join, and reading a
//! lookup's contents is a plain `SELECT` from its table.
//!
//! **Everything keys on `Id`, never `Name`.** Other tables reference a lookup
//! value by its `Id` (a Guid), so a migration must preserve that Guid or it
//! breaks foreign keys on the target. The generated migration is therefore an
//! idempotent `INSERT … ON CONFLICT ("Id") DO UPDATE`, which keeps the same Guid
//! on both sides and can be re-run without creating duplicates.
//!
//! **Reads reuse `sql.rs`.** All SQL goes through `clio execute-sql-script`,
//! exactly like the SQL screen, including its failure detection — clio exits 0
//! and prints no `[ERR]` when the database itself rejects a query, so the output
//! has to be read for the error. `sql::is_failure` / `sql::friendly_error` own
//! that logic and are shared rather than duplicated here.
//!
//! **Scope (v1).** Columns compared and migrated are the `BaseLookup` set
//! (`Id`, `Name`, `Description`); lookups with extra custom columns keep only
//! those three for now (a later version can introspect columns per table). The
//! migration SQL targets PostgreSQL (`ON CONFLICT`), which is what the live
//! environments run.

use crate::envstate::{DiffReport, DiffRow};
use crate::jobs::JobState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

// ------------------------------------------------------------------- types

/// One registered lookup: its display name, backing table, owning package, and
/// whether the table carries a `Description` column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupInfo {
    /// The lookup's display name from `SysLookup."Name"`.
    pub name: String,
    /// The backing entity table name from `SysSchema."Name"`.
    pub table: String,
    /// The package that owns the entity schema, for optional "my lookups" filtering.
    pub package: String,
    /// Whether `<table>` has a `Description` column (a few lookups do not).
    pub has_description: bool,
}

/// A single lookup value row, reduced to the columns v1 compares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupRow {
    pub id: String,
    pub name: String,
    /// Absent when the table has no `Description` column.
    pub description: Option<String>,
}

/// The captured contents of one lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupTable {
    /// The lookup's display name (for a friendlier UI than the raw table name).
    pub title: String,
    pub rows: Vec<LookupRow>,
}

/// Every lookup table's contents for one environment, captured to disk so the
/// comparison is local and instant — the same model as `envstate` snapshots.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupSnapshot {
    pub env: String,
    pub captured_at: u64,
    /// Table name → its contents.
    pub lookups: BTreeMap<String, LookupTable>,
}

/// A listed lookup snapshot file, for the capture/compare UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupSnapshotInfo {
    pub env: String,
    pub captured_at: u64,
    pub size_bytes: u64,
    pub lookup_count: usize,
}

// -------------------------------------------------------------- pure: SQL

/// Enumerate every lookup with its backing table, owning package, and whether
/// the table has a `Description` column.
///
/// `SysLookup."SysEntitySchemaUId"` joins to `SysSchema."UId"` (schema
/// references are by UId, not Id). The `information_schema` existence check is
/// standard SQL and works through cliogate.
///
/// Lookups whose backing table lacks a `Name` column are excluded outright:
/// a handful of registered lookups point at system views (for example
/// `VwSysSSPEntitySchemaAccessList`) whose column set does not follow the
/// Id/Name/Description shape. One such table inside the capture `UNION ALL`
/// makes PostgreSQL reject the whole statement with SQLSTATE 42703, so they
/// must never reach the capture query — and v1's Id/Name/Description model
/// cannot compare or migrate them anyway. The filter also drops registry rows
/// whose table does not exist in the database at all.
pub fn enumeration_sql() -> String {
    r#"SELECT l."Name" AS "Name",
       s."Name" AS "Table",
       COALESCE(p."Name", '') AS "Package",
       CASE WHEN EXISTS (
         SELECT 1 FROM information_schema.columns c
         WHERE c.table_name = s."Name" AND c.column_name = 'Description'
       ) THEN '1' ELSE '0' END AS "HasDescription"
FROM "SysLookup" l
JOIN "SysSchema" s ON s."UId" = l."SysEntitySchemaUId"
LEFT JOIN "SysPackage" p ON p."Id" = s."SysPackageId"
WHERE EXISTS (
  SELECT 1 FROM information_schema.columns c
  WHERE c.table_name = s."Name" AND c.column_name = 'Name'
)
ORDER BY l."Name""#
        .to_string()
}

/// Whether `name` is a bare identifier safe to splice into SQL unquoted-content.
///
/// Creatio schema and table names are alphanumerics (e.g. `CasePriority`,
/// `UsrMyLookup`). Anything else is refused rather than escaped, so a table name
/// can never carry SQL into a generated statement.
pub fn is_safe_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The `SELECT` that reads one lookup table's rows.
pub fn select_table_sql(table: &str, has_description: bool) -> String {
    let description = if has_description {
        "\"Description\""
    } else {
        "CAST(NULL AS text)"
    };
    format!(
        "SELECT CAST(\"Id\" AS text) AS \"Id\", \"Name\" AS \"Name\", {description} AS \"Description\" FROM \"{table}\""
    )
}

/// One `UNION ALL` query reading every lookup's rows in a single round trip,
/// each row tagged with its table in the `__lk` column.
///
/// Tables whose names are not bare identifiers are skipped (they cannot occur in
/// stock Creatio, but the guard is absolute). An empty table simply contributes
/// no rows — the caller seeds the snapshot from the enumeration so an empty
/// lookup is still recorded rather than read as absent.
pub fn capture_sql(lookups: &[LookupInfo]) -> String {
    lookups
        .iter()
        .filter(|info| is_safe_identifier(&info.table))
        .map(|info| {
            let description = if info.has_description {
                "\"Description\""
            } else {
                "CAST(NULL AS text)"
            };
            format!(
                "SELECT '{table}' AS \"__lk\", CAST(\"Id\" AS text) AS \"Id\", \"Name\" AS \"Name\", {description} AS \"Description\" FROM \"{table}\"",
                table = info.table
            )
        })
        .collect::<Vec<_>>()
        .join("\nUNION ALL\n")
}

/// Escape a value for a single-quoted SQL string literal.
pub fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Build an idempotent upsert for one lookup table.
///
/// Keyed on `Id` so re-running never duplicates and existing rows keep their
/// Guid — the property foreign keys depend on. PostgreSQL `ON CONFLICT`.
pub fn build_upsert_sql(table: &str, has_description: bool, rows: &[LookupRow]) -> String {
    if rows.is_empty() || !is_safe_identifier(table) {
        return String::new();
    }
    let columns = if has_description {
        "(\"Id\", \"Name\", \"Description\")"
    } else {
        "(\"Id\", \"Name\")"
    };
    let values: Vec<String> = rows
        .iter()
        .map(|row| {
            let id = escape_literal(&row.id);
            let name = escape_literal(&row.name);
            if has_description {
                let description = match &row.description {
                    Some(text) => format!("'{}'", escape_literal(text)),
                    None => "NULL".to_string(),
                };
                format!("('{id}', '{name}', {description})")
            } else {
                format!("('{id}', '{name}')")
            }
        })
        .collect();
    let set = if has_description {
        "\"Name\" = EXCLUDED.\"Name\", \"Description\" = EXCLUDED.\"Description\""
    } else {
        "\"Name\" = EXCLUDED.\"Name\""
    };
    format!(
        "INSERT INTO \"{table}\" {columns} VALUES\n{}\nON CONFLICT (\"Id\") DO UPDATE SET {set};",
        values.join(",\n")
    )
}

// ------------------------------------------------------------- pure: diff

fn status_of(source: bool, target: bool, differs: bool) -> &'static str {
    match (source, target) {
        (true, true) if differs => "different",
        (true, true) => "same",
        (true, false) => "missingTarget",
        (false, true) => "missingSource",
        (false, false) => "same",
    }
}

/// Compare two lookup snapshots into the same `DiffReport` shape the Compare
/// screen already renders. Each top row is a lookup; its `detail` rows are the
/// individual values that differ, keyed on `Id`.
pub fn diff_snapshots(source: &LookupSnapshot, target: &LookupSnapshot) -> DiffReport {
    let mut names: Vec<&String> = source.lookups.keys().chain(target.lookups.keys()).collect();
    names.sort();
    names.dedup();

    let mut rows = Vec::new();
    for table in names {
        let left = source.lookups.get(table);
        let right = target.lookups.get(table);

        let detail = value_diffs(
            left.map(|t| t.rows.as_slice()).unwrap_or(&[]),
            right.map(|t| t.rows.as_slice()).unwrap_or(&[]),
        );
        let differs = left.is_some() && right.is_some() && !detail.is_empty();
        let status = status_of(left.is_some(), right.is_some(), differs);

        // Prefer the human-friendly display name; fall back to the table name.
        let title = left
            .map(|t| t.title.clone())
            .filter(|t| !t.is_empty())
            .or_else(|| right.map(|t| t.title.clone()).filter(|t| !t.is_empty()))
            .unwrap_or_else(|| table.clone());

        rows.push(DiffRow {
            category: "lookup".to_string(),
            key: title,
            source: left.map(|t| format!("{} values", t.rows.len())),
            target: right.map(|t| format!("{} values", t.rows.len())),
            status: status.to_string(),
            sensitive: false,
            detail: if differs { detail } else { Vec::new() },
        });
    }

    let mut counts = BTreeMap::new();
    let differing = rows.iter().filter(|row| row.status != "same").count();
    if differing > 0 {
        counts.insert("lookup".to_string(), differing);
    }

    DiffReport {
        source_env: source.env.clone(),
        target_env: target.env.clone(),
        source_captured_at: source.captured_at,
        target_captured_at: target.captured_at,
        rows,
        counts,
    }
}

/// Per-value differences between two lookups, keyed on `Id`. A row appears only
/// when it is absent on one side or its `Name`/`Description` differs. The `Name`
/// travels in the source/target cells because it is what a human reads; a
/// `Description`-only change still surfaces the row as different.
fn value_diffs(source: &[LookupRow], target: &[LookupRow]) -> Vec<DiffRow> {
    let index = |rows: &[LookupRow]| {
        rows.iter()
            .map(|row| (row.id.clone(), row.clone()))
            .collect::<BTreeMap<String, LookupRow>>()
    };
    let left = index(source);
    let right = index(target);

    let mut ids: Vec<&String> = left.keys().chain(right.keys()).collect();
    ids.sort();
    ids.dedup();

    let mut out = Vec::new();
    for id in ids {
        let a = left.get(id);
        let b = right.get(id);
        let differs = matches!((a, b), (Some(x), Some(y)) if x != y);
        let status = status_of(a.is_some(), b.is_some(), differs);
        if status == "same" {
            continue;
        }
        out.push(DiffRow {
            category: "lookupValue".to_string(),
            key: id.clone(),
            source: a.map(|row| row.name.clone()),
            target: b.map(|row| row.name.clone()),
            status: status.to_string(),
            sensitive: false,
            detail: Vec::new(),
        });
    }
    out
}

// --------------------------------------------------------------- IO: reads

fn temp_path(ext: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("devhub-refdata-{n}.{ext}"))
}

/// Run a `SELECT` through clio and parse the CSV it writes. Uncapped, unlike the
/// SQL screen's grid — a captured lookup must be complete to compare or migrate.
/// A query that matches nothing writes no file, which is an empty result, not a
/// failure; a database error is caught by the shared `sql::is_failure`.
fn run_select(env: &str, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let env = env.trim();
    if env.is_empty() {
        return Err("Choose an environment.".to_string());
    }
    let sql_path = temp_path("sql");
    let csv_path = temp_path("csv");
    std::fs::write(&sql_path, sql).map_err(|e| format!("Could not stage the query: {e}"))?;
    let sql_str = sql_path.to_string_lossy().to_string();
    let csv_str = csv_path.to_string_lossy().to_string();

    let result = crate::clio::clio_capture(&[
        "execute-sql-script", "-f", &sql_str, "-e", env, "-v", "csv", "-d", &csv_str,
    ]);
    let _ = std::fs::remove_file(&sql_path);

    let (code, out) = result?;
    if crate::sql::is_failure(code, &out) {
        let _ = std::fs::remove_file(&csv_path);
        return Err(crate::sql::friendly_error(&out));
    }
    if !csv_path.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let parsed = parse_result_csv(&csv_path);
    let _ = std::fs::remove_file(&csv_path);
    parsed
}

/// Parse clio's semicolon-delimited CSV. No row cap.
fn parse_result_csv(path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("Could not read the result: {e}"))?;
    let columns: Vec<String> = reader
        .headers()
        .map_err(|e| format!("Could not read column headers: {e}"))?
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("Malformed result row: {e}"))?;
        rows.push(record.iter().map(|s| s.to_string()).collect());
    }
    Ok((columns, rows))
}

fn column_index(columns: &[String], name: &str) -> Option<usize> {
    columns.iter().position(|column| column == name)
}

/// Turn a `SELECT Id, Name, Description` result into rows, dropping any without
/// an `Id`.
fn rows_from_result(columns: &[String], data: Vec<Vec<String>>) -> Vec<LookupRow> {
    let (Some(id_at), Some(name_at)) = (column_index(columns, "Id"), column_index(columns, "Name"))
    else {
        return Vec::new();
    };
    let desc_at = column_index(columns, "Description");
    data.into_iter()
        .filter_map(|record| {
            let id = record.get(id_at)?.trim().to_string();
            if id.is_empty() {
                return None;
            }
            let name = record.get(name_at).cloned().unwrap_or_default();
            let description = desc_at
                .and_then(|at| record.get(at))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty());
            Some(LookupRow { id, name, description })
        })
        .collect()
}

/// Drop registry entries whose table was already listed, keeping the first.
///
/// Two `SysLookup` rows can point at the same entity schema (seen live:
/// `LeadType` registered twice). Snapshots key on the table name, so a
/// duplicate contributes nothing — but inside `capture_sql` it duplicates the
/// table's `SELECT`, and every row would be captured twice.
pub fn dedupe_by_table(infos: Vec<LookupInfo>) -> Vec<LookupInfo> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    infos.into_iter().filter(|info| seen.insert(info.table.clone())).collect()
}

/// Read the lookup registry of `env`.
fn enumerate(env: &str) -> Result<Vec<LookupInfo>, String> {
    let (columns, data) = run_select(env, &enumeration_sql())?;
    let name_at = column_index(&columns, "Name");
    let table_at = column_index(&columns, "Table");
    let package_at = column_index(&columns, "Package");
    let desc_at = column_index(&columns, "HasDescription");
    let (Some(name_at), Some(table_at)) = (name_at, table_at) else {
        return Err("The lookup registry query returned an unexpected shape.".to_string());
    };
    let infos: Vec<LookupInfo> = data
        .into_iter()
        .filter_map(|record| {
            let name = record.get(name_at).cloned().unwrap_or_default();
            let table = record.get(table_at).cloned().unwrap_or_default();
            if table.is_empty() {
                return None;
            }
            let package = package_at.and_then(|at| record.get(at)).cloned().unwrap_or_default();
            let has_description =
                desc_at.and_then(|at| record.get(at)).map(|value| value == "1").unwrap_or(false);
            Some(LookupInfo { name, table, package, has_description })
        })
        .collect();
    Ok(dedupe_by_table(infos))
}

// ---------------------------------------------------------- snapshot store

/// Lookup snapshots live beside the `envstate` snapshots but in their own
/// `.lookups.json` files, listed and deleted independently. They hold lookup
/// values, which are far less likely than system settings to be secrets, but the
/// same delete-when-done discipline applies.
fn snapshot_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("No application data directory: {error}"))?
        .join("snapshots");
    std::fs::create_dir_all(&dir).map_err(|error| format!("Could not create {dir:?}: {error}"))?;
    Ok(dir)
}

fn safe_env(env: &str) -> Result<String, String> {
    let safe: String = env
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if safe.is_empty() {
        return Err("Select an environment first.".to_string());
    }
    Ok(safe)
}

fn snapshot_path(app: &AppHandle, env: &str) -> Result<PathBuf, String> {
    Ok(snapshot_dir(app)?.join(format!("{}.lookups.json", safe_env(env)?)))
}

fn modified_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

fn read_snapshot(app: &AppHandle, env: &str) -> Result<LookupSnapshot, String> {
    let path = snapshot_path(app, env)?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| format!("No lookup snapshot of {env} yet — capture it first."))?;
    let mut snapshot: LookupSnapshot =
        serde_json::from_str(&raw).map_err(|error| format!("Could not read the snapshot: {error}"))?;
    if snapshot.captured_at == 0 {
        snapshot.captured_at = modified_ms(&path);
    }
    Ok(snapshot)
}

// ------------------------------------------------------------- commands

/// List every lookup registered in `env` — the picker for the Migration screen.
#[tauri::command]
pub fn list_lookups(env: String) -> Result<Vec<LookupInfo>, String> {
    enumerate(env.trim())
}

/// List captured lookup snapshots.
#[tauri::command]
pub fn list_lookup_snapshots(app: AppHandle) -> Result<Vec<LookupSnapshotInfo>, String> {
    let dir = snapshot_dir(&app)?;
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|error| error.to_string())?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        let Some(stem) = name.strip_suffix(".lookups.json") else { continue };
        let lookup_count = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LookupSnapshot>(&raw).ok())
            .map(|snapshot| snapshot.lookups.len())
            .unwrap_or(0);
        found.push(LookupSnapshotInfo {
            env: stem.to_string(),
            captured_at: modified_ms(&path),
            size_bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            lookup_count,
        });
    }
    found.sort_by(|a, b| a.env.to_lowercase().cmp(&b.env.to_lowercase()));
    Ok(found)
}

#[tauri::command]
pub fn delete_lookup_snapshot(app: AppHandle, env: String) -> Result<(), String> {
    let path = snapshot_path(&app, &env)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Could not delete the snapshot: {error}"))?;
    }
    Ok(())
}

/// Capture every lookup's contents in `env` to a local snapshot.
///
/// Read-only, but run as a job because a cloud environment's lookups take a
/// while over the network. Not cancellable in v1: the read is a single clio
/// invocation whose output is parsed only after it returns.
#[tauri::command]
pub fn capture_lookups(
    app: AppHandle,
    jobs: State<'_, JobState>,
    env: String,
) -> Result<String, String> {
    let env = env.trim().to_string();
    let path = snapshot_path(&app, &env)?;
    let id = jobs.create_job(
        &app,
        "capture-lookups",
        Some(env.clone()),
        format!("capture lookups of {env}"),
    );
    let lock = jobs.env_lock(Some(&env));
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !state.mark_running_phase(&app, &job_id, "reading lookup registry", false) {
            return;
        }
        let started = std::time::Instant::now();

        let infos = match enumerate(&env) {
            Ok(infos) if !infos.is_empty() => infos,
            Ok(_) => {
                state.log(&app, &job_id, "No lookups were found in this environment.".to_string());
                state.finish(&app, &job_id, Some(1));
                return;
            }
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
                return;
            }
        };
        state.log(&app, &job_id, format!("Found {} lookups. Reading their values…", infos.len()));
        if !state.set_phase(&app, &job_id, "reading lookup values", false) {
            return;
        }

        let sql = capture_sql(&infos);
        let sql_path = temp_path("sql");
        let csv_path = temp_path("csv");
        if let Err(error) = std::fs::write(&sql_path, &sql) {
            state.log(&app, &job_id, format!("Could not stage the query: {error}"));
            state.finish(&app, &job_id, Some(1));
            return;
        }
        let args = vec![
            "execute-sql-script".to_string(),
            "-f".to_string(),
            sql_path.to_string_lossy().to_string(),
            "-e".to_string(),
            env.clone(),
            "-v".to_string(),
            "csv".to_string(),
            "-d".to_string(),
            csv_path.to_string_lossy().to_string(),
        ];
        let outcome = state.stream_process(&app, &job_id, "clio", &args, None, &[]);
        let _ = std::fs::remove_file(&sql_path);

        match outcome {
            Ok(code) if code != 0 => {
                let _ = std::fs::remove_file(&csv_path);
                state.finish(&app, &job_id, Some(code));
            }
            Ok(_) => {
                // A healthy environment always has system lookups, so no file at
                // all means the read did not succeed (missing cliogate, or a
                // rejected query clio reported with a zero exit).
                if !csv_path.exists() {
                    state.log(
                        &app,
                        &job_id,
                        "No lookup data was returned. The environment may be missing the cliogate \
                         helper (clio install-gate), or the query was rejected."
                            .to_string(),
                    );
                    state.finish(&app, &job_id, Some(1));
                    return;
                }
                match parse_result_csv(&csv_path) {
                    Ok((columns, data)) => {
                        let _ = std::fs::remove_file(&csv_path);
                        let snapshot = build_snapshot(&env, &infos, &columns, data);
                        match serde_json::to_string_pretty(&snapshot) {
                            Ok(json) => {
                                if let Err(error) = std::fs::write(&path, json) {
                                    state.log(
                                        &app,
                                        &job_id,
                                        format!("Could not write the snapshot: {error}"),
                                    );
                                    state.finish(&app, &job_id, Some(1));
                                    return;
                                }
                                record_duration(&app, &env, started.elapsed().as_millis() as u64);
                                let values: usize =
                                    snapshot.lookups.values().map(|table| table.rows.len()).sum();
                                state.log(
                                    &app,
                                    &job_id,
                                    format!(
                                        "Captured {} lookups ({values} values). This snapshot holds \
                                         lookup data — delete it from the Compare screen when done.",
                                        snapshot.lookups.len()
                                    ),
                                );
                                state.finish(&app, &job_id, Some(0));
                            }
                            Err(error) => {
                                state.log(&app, &job_id, format!("Could not serialize: {error}"));
                                state.finish(&app, &job_id, Some(1));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = std::fs::remove_file(&csv_path);
                        state.log(&app, &job_id, error);
                        state.finish(&app, &job_id, Some(1));
                    }
                }
            }
            Err(error) => {
                let _ = std::fs::remove_file(&csv_path);
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    Ok(id)
}

/// Assemble a snapshot from the enumeration (which seeds every table, so an
/// empty lookup is recorded) and the `UNION ALL` result grouped by table.
fn build_snapshot(
    env: &str,
    infos: &[LookupInfo],
    columns: &[String],
    data: Vec<Vec<String>>,
) -> LookupSnapshot {
    let mut lookups: BTreeMap<String, LookupTable> = infos
        .iter()
        .map(|info| (info.table.clone(), LookupTable { title: info.name.clone(), rows: Vec::new() }))
        .collect();

    let lk_at = column_index(columns, "__lk");
    let id_at = column_index(columns, "Id");
    let name_at = column_index(columns, "Name");
    let desc_at = column_index(columns, "Description");
    if let (Some(lk_at), Some(id_at), Some(name_at)) = (lk_at, id_at, name_at) {
        for record in data {
            let Some(table) = record.get(lk_at) else { continue };
            let Some(id) = record.get(id_at) else { continue };
            let id = id.trim().to_string();
            if id.is_empty() {
                continue;
            }
            let name = record.get(name_at).cloned().unwrap_or_default();
            let description = desc_at
                .and_then(|at| record.get(at))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty());
            if let Some(entry) = lookups.get_mut(table) {
                entry.rows.push(LookupRow { id, name, description });
            }
        }
    }

    LookupSnapshot {
        env: env.to_string(),
        captured_at: crate::jobs::now_ms(),
        lookups,
    }
}

/// Compare two captured lookup snapshots. Reads only from disk — instant, and
/// nothing touches an environment.
#[tauri::command]
pub fn diff_lookups(
    app: AppHandle,
    source_env: String,
    target_env: String,
) -> Result<DiffReport, String> {
    if source_env.trim() == target_env.trim() {
        return Err("Choose two different environments to compare.".to_string());
    }
    let source = read_snapshot(&app, &source_env)?;
    let target = read_snapshot(&app, &target_env)?;
    Ok(diff_snapshots(&source, &target))
}

/// Read one lookup table's current rows.
fn read_table_rows(env: &str, info: &LookupInfo) -> Result<Vec<LookupRow>, String> {
    let (columns, data) = run_select(env, &select_table_sql(&info.table, info.has_description))?;
    Ok(rows_from_result(&columns, data))
}

/// One lookup to migrate: its registry info plus the source rows that define
/// what the target should hold.
struct TablePlan {
    info: LookupInfo,
    source_rows: Vec<LookupRow>,
}

/// Resolve the requested tables against the **source** registry and read their
/// current rows. Fails closed on an unknown or unsafe table name.
fn plan_migration(source_env: &str, tables: &[String]) -> Result<Vec<TablePlan>, String> {
    if tables.is_empty() {
        return Err("Select at least one lookup to migrate.".to_string());
    }
    let by_table: BTreeMap<String, LookupInfo> =
        enumerate(source_env)?.into_iter().map(|info| (info.table.clone(), info)).collect();

    let mut plans = Vec::new();
    for table in tables {
        if !is_safe_identifier(table) {
            return Err(format!("Refusing to migrate an unsafe table name: {table}"));
        }
        let info = by_table
            .get(table)
            .ok_or_else(|| format!("{table} is not a lookup in {source_env}."))?
            .clone();
        let source_rows = read_table_rows(source_env, &info)?;
        plans.push(TablePlan { info, source_rows });
    }
    Ok(plans)
}

/// The forward migration: upsert each lookup's source rows onto the target.
fn forward_sql(plans: &[TablePlan]) -> String {
    let mut out = String::new();
    for plan in plans {
        let statement = build_upsert_sql(&plan.info.table, plan.info.has_description, &plan.source_rows);
        if !statement.is_empty() {
            out.push_str(&format!("-- {} ({} rows)\n", plan.info.name, plan.source_rows.len()));
            out.push_str(&statement);
            out.push_str("\n\n");
        }
    }
    out
}

/// Delete the given ids from a table — the inverse of an insert.
fn build_delete_sql(table: &str, ids: &[&str]) -> String {
    if ids.is_empty() || !is_safe_identifier(table) {
        return String::new();
    }
    let list = ids.iter().map(|id| format!("'{}'", escape_literal(id))).collect::<Vec<_>>().join(", ");
    format!("DELETE FROM \"{table}\" WHERE \"Id\" IN ({list});")
}

/// A runnable rollback for the forward migration, built from the target's
/// **current** state. For each id the migration will touch: if the target
/// already has it, restore its current values (undo an update); if not, delete
/// it (undo an insert). Re-running this file returns the affected tables to
/// exactly where they were. Reads the target, so it is done before the write.
fn rollback_sql(target_env: &str, plans: &[TablePlan]) -> Result<String, String> {
    let mut out = String::new();
    for plan in plans {
        let current = read_table_rows(target_env, &plan.info)?;
        let current_by_id: BTreeMap<String, LookupRow> =
            current.into_iter().map(|row| (row.id.clone(), row)).collect();

        let mut restore = Vec::new();
        let mut delete_ids: Vec<String> = Vec::new();
        for row in &plan.source_rows {
            match current_by_id.get(&row.id) {
                Some(existing) => restore.push(existing.clone()),
                None => delete_ids.push(row.id.clone()),
            }
        }

        let upsert = build_upsert_sql(&plan.info.table, plan.info.has_description, &restore);
        let delete_refs: Vec<&str> = delete_ids.iter().map(String::as_str).collect();
        let delete = build_delete_sql(&plan.info.table, &delete_refs);
        if !upsert.is_empty() || !delete.is_empty() {
            out.push_str(&format!(
                "-- rollback {} (restore {} / remove {})\n",
                plan.info.name,
                restore.len(),
                delete_ids.len()
            ));
            if !upsert.is_empty() {
                out.push_str(&upsert);
                out.push('\n');
            }
            if !delete.is_empty() {
                out.push_str(&delete);
                out.push('\n');
            }
            out.push('\n');
        }
    }
    Ok(out)
}

/// Build the migration SQL for the selected lookups by reading the **source**
/// environment live, so the preview reflects current data rather than a possibly
/// stale snapshot. Read-only: this only produces the SQL text for review;
/// `migrate_lookups` applies it to the target as a mutating job.
#[tauri::command]
pub fn build_lookup_migration(source_env: String, tables: Vec<String>) -> Result<String, String> {
    let plans = plan_migration(source_env.trim(), &tables)?;
    let out = forward_sql(&plans);
    if out.is_empty() {
        return Err("The selected lookups have no rows to migrate.".to_string());
    }
    Ok(out)
}

/// Directory holding applied/rollback migration scripts, kept as a record.
fn migrations_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("No application data directory: {error}"))?
        .join("migrations");
    std::fs::create_dir_all(&dir).map_err(|error| format!("Could not create {dir:?}: {error}"))?;
    Ok(dir)
}

/// Apply the selected lookups' source rows onto the target as an idempotent
/// upsert. **Mutates the target** — env-locked on both sides and non-cancellable
/// once the write begins, the same discipline as package deployment.
///
/// Unless `skip_backup` is set, the target's current state for the affected
/// tables is read first and written as a runnable rollback script, whose path is
/// logged. The write goes through `clio_capture` rather than `stream_process` so
/// clio's output can be inspected: `execute-sql-script` exits 0 even when the
/// database rejects the SQL, so the exit code alone cannot be trusted.
#[tauri::command]
pub fn migrate_lookups(
    app: AppHandle,
    jobs: State<'_, JobState>,
    source_env: String,
    target_env: String,
    tables: Vec<String>,
    skip_backup: bool,
) -> Result<String, String> {
    let source_env = source_env.trim().to_string();
    let target_env = target_env.trim().to_string();
    if source_env == target_env {
        return Err("Choose a different target environment.".to_string());
    }
    if tables.is_empty() {
        return Err("Select at least one lookup to migrate.".to_string());
    }
    let environments = crate::clio::list_environments()?;
    for name in [&source_env, &target_env] {
        if !environments.iter().any(|environment| &environment.name == name) {
            return Err(format!("Environment {name} is not registered in clio."));
        }
    }

    let id = jobs.create_job(
        &app,
        "migrate-lookups",
        Some(target_env.clone()),
        format!("migrate {} lookup(s): {source_env} → {target_env}", tables.len()),
    );
    // Acquire both environment locks in a stable order to avoid deadlocking
    // against a deployment running the other direction.
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
        if !state.mark_running_phase(&app, &job_id, "reading source lookups", true) {
            return;
        }

        let plans = match plan_migration(&source_env, &tables) {
            Ok(plans) => plans,
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
                return;
            }
        };
        let forward = forward_sql(&plans);
        if forward.is_empty() {
            state.log(&app, &job_id, "The selected lookups have no rows to migrate.".to_string());
            state.finish(&app, &job_id, Some(0));
            return;
        }
        let total_rows: usize = plans.iter().map(|plan| plan.source_rows.len()).sum();

        // Back up the target's current state as a runnable rollback before writing.
        if !skip_backup {
            if !state.set_phase(&app, &job_id, "backing up target", true) {
                return;
            }
            match rollback_sql(&target_env, &plans) {
                Ok(rollback) => {
                    let stamp = crate::jobs::now_ms();
                    match migrations_dir(&app) {
                        Ok(dir) => {
                            let rollback_path = dir.join(format!("rollback-{target_env}-{stamp}.sql"));
                            let applied_path = dir.join(format!("applied-{target_env}-{stamp}.sql"));
                            if let Err(error) = std::fs::write(&rollback_path, &rollback) {
                                state.log(&app, &job_id, format!("Could not write the rollback: {error}"));
                                state.finish(&app, &job_id, Some(1));
                                return;
                            }
                            let _ = std::fs::write(&applied_path, &forward);
                            state.log(
                                &app,
                                &job_id,
                                format!(
                                    "Backup written. To undo this migration, run this file against \
                                     {target_env}:\n{}",
                                    rollback_path.to_string_lossy()
                                ),
                            );
                        }
                        Err(error) => {
                            state.log(&app, &job_id, error);
                            state.finish(&app, &job_id, Some(1));
                            return;
                        }
                    }
                }
                Err(error) => {
                    state.log(&app, &job_id, format!("Could not read the target for backup: {error}"));
                    state.finish(&app, &job_id, Some(1));
                    return;
                }
            }
        }

        // The write phase is unsafe: no cancellation past this point.
        if !state.set_phase(&app, &job_id, "writing to target", false) {
            return;
        }
        // A single script runs as one implicit transaction on PostgreSQL; the
        // explicit BEGIN/COMMIT makes the all-or-nothing intent unmistakable.
        let script = format!("BEGIN;\n{forward}COMMIT;\n");
        let sql_path = temp_path("sql");
        if let Err(error) = std::fs::write(&sql_path, &script) {
            state.log(&app, &job_id, format!("Could not stage the migration: {error}"));
            state.finish(&app, &job_id, Some(1));
            return;
        }
        let sql_str = sql_path.to_string_lossy().to_string();
        state.log(
            &app,
            &job_id,
            format!("Applying {total_rows} row(s) across {} lookup(s) to {target_env}…", plans.len()),
        );

        let result = crate::clio::clio_capture(&["execute-sql-script", "-f", &sql_str, "-e", &target_env]);
        let _ = std::fs::remove_file(&sql_path);

        match result {
            Ok((code, out)) => {
                for line in out.lines().filter(|line| !line.trim().is_empty()) {
                    state.log(&app, &job_id, line.to_string());
                }
                if crate::sql::is_failure(code, &out) {
                    state.log(&app, &job_id, format!("Migration failed: {}", crate::sql::friendly_error(&out)));
                    if !skip_backup {
                        state.log(
                            &app,
                            &job_id,
                            "Nothing was committed if the transaction rolled back; the rollback \
                             file above restores the target either way."
                                .to_string(),
                        );
                    }
                    state.finish(&app, &job_id, Some(if code == 0 { 1 } else { code }));
                } else {
                    state.log(
                        &app,
                        &job_id,
                        format!("✓ Migrated {total_rows} row(s) across {} lookup(s) to {target_env}.", plans.len()),
                    );
                    state.finish(&app, &job_id, Some(0));
                }
            }
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    Ok(id)
}

// ------------------------------------------------------------- durations

fn durations_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(snapshot_dir(app)?.join("lookup-durations.json"))
}

fn read_durations(app: &AppHandle) -> BTreeMap<String, u64> {
    durations_file(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn record_duration(app: &AppHandle, env: &str, elapsed_ms: u64) {
    let mut all = read_durations(app);
    all.insert(env.to_string(), elapsed_ms);
    if let (Ok(path), Ok(json)) = (durations_file(app), serde_json::to_string_pretty(&all)) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, name: &str, description: Option<&str>) -> LookupRow {
        LookupRow { id: id.to_string(), name: name.to_string(), description: description.map(String::from) }
    }

    fn snapshot(env: &str, tables: &[(&str, &str, &[LookupRow])]) -> LookupSnapshot {
        LookupSnapshot {
            env: env.to_string(),
            captured_at: 1,
            lookups: tables
                .iter()
                .map(|(table, title, rows)| {
                    (table.to_string(), LookupTable { title: title.to_string(), rows: rows.to_vec() })
                })
                .collect(),
        }
    }

    #[test]
    fn identifier_guard_rejects_anything_that_could_carry_sql() {
        assert!(is_safe_identifier("CasePriority"));
        assert!(is_safe_identifier("UsrMy_Lookup1"));
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("Case Priority"));
        assert!(!is_safe_identifier("Case\";DROP"));
        assert!(!is_safe_identifier("Case'--"));
    }

    #[test]
    fn enumeration_excludes_tables_without_a_name_column() {
        // A lookup backed by a Vw* system view (seen live:
        // VwSysSSPEntitySchemaAccessList) has no "Name" column and poisons the
        // whole capture UNION with SQLSTATE 42703, so the registry query must
        // filter such tables out before they ever reach capture_sql.
        let sql = enumeration_sql();
        assert!(sql.contains("WHERE EXISTS"));
        assert!(sql.contains("c.column_name = 'Name'"));
    }

    #[test]
    fn dedupe_by_table_keeps_the_first_entry() {
        // Seen live: LeadType registered twice in SysLookup; without deduping,
        // its SELECT appears twice in the UNION and every row is captured twice.
        let infos = vec![
            LookupInfo { name: "Lead type".into(), table: "LeadType".into(), package: "Base".into(), has_description: true },
            LookupInfo { name: "Lead type (again)".into(), table: "LeadType".into(), package: "Other".into(), has_description: true },
            LookupInfo { name: "Flag".into(), table: "UsrFlag".into(), package: "Custom".into(), has_description: false },
        ];
        let deduped = dedupe_by_table(infos);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].name, "Lead type");
        assert_eq!(deduped[1].table, "UsrFlag");
    }

    #[test]
    fn capture_sql_unions_only_safe_tables() {
        let infos = vec![
            LookupInfo { name: "Case priority".into(), table: "CasePriority".into(), package: "Base".into(), has_description: true },
            LookupInfo { name: "Bad".into(), table: "Bad Table".into(), package: "X".into(), has_description: false },
            LookupInfo { name: "Flag".into(), table: "UsrFlag".into(), package: "Custom".into(), has_description: false },
        ];
        let sql = capture_sql(&infos);
        assert!(sql.contains("FROM \"CasePriority\""));
        assert!(sql.contains("'CasePriority' AS \"__lk\""));
        // The description-less table selects a NULL description so the UNION stays uniform.
        assert!(sql.contains("CAST(NULL AS text) AS \"Description\" FROM \"UsrFlag\""));
        // The unsafe table name never reaches the SQL.
        assert!(!sql.contains("Bad Table"));
        assert_eq!(sql.matches("UNION ALL").count(), 1);
    }

    #[test]
    fn upsert_is_idempotent_and_keyed_on_id() {
        let rows = vec![row("a1", "High", Some("Urgent")), row("a2", "Low", None)];
        let sql = build_upsert_sql("CasePriority", true, &rows);
        assert!(sql.starts_with("INSERT INTO \"CasePriority\" (\"Id\", \"Name\", \"Description\") VALUES"));
        assert!(sql.contains("('a1', 'High', 'Urgent')"));
        // A missing description becomes NULL, not an empty string.
        assert!(sql.contains("('a2', 'Low', NULL)"));
        assert!(sql.contains("ON CONFLICT (\"Id\") DO UPDATE SET \"Name\" = EXCLUDED.\"Name\", \"Description\" = EXCLUDED.\"Description\";"));
    }

    #[test]
    fn upsert_without_description_omits_the_column() {
        let rows = vec![row("a1", "Yes", None)];
        let sql = build_upsert_sql("UsrFlag", false, &rows);
        assert!(sql.contains("(\"Id\", \"Name\") VALUES"));
        assert!(sql.contains("('a1', 'Yes')"));
        assert!(!sql.contains("Description"));
    }

    #[test]
    fn upsert_escapes_quotes_in_values() {
        let rows = vec![row("id1", "O'Brien", Some("it's fine"))];
        let sql = build_upsert_sql("UsrName", true, &rows);
        assert!(sql.contains("('id1', 'O''Brien', 'it''s fine')"));
    }

    #[test]
    fn upsert_of_nothing_is_empty() {
        assert!(build_upsert_sql("CasePriority", true, &[]).is_empty());
        assert!(build_upsert_sql("Bad Table", true, &[row("a", "b", None)]).is_empty());
    }

    #[test]
    fn diff_flags_changed_added_and_removed_values() {
        let source = snapshot(
            "dev",
            &[("CasePriority", "Case priority", &[row("a1", "High", None), row("a2", "Low", None)])],
        );
        let target = snapshot(
            "pre",
            &[("CasePriority", "Case priority", &[row("a1", "Critical", None), row("a3", "Trivial", None)])],
        );
        let report = diff_snapshots(&source, &target);
        assert_eq!(report.rows.len(), 1);
        let lookup = &report.rows[0];
        assert_eq!(lookup.category, "lookup");
        assert_eq!(lookup.key, "Case priority");
        assert_eq!(lookup.status, "different");
        assert_eq!(report.counts["lookup"], 1);

        // a1 changed name, a2 only in source, a3 only in target — three detail rows.
        assert_eq!(lookup.detail.len(), 3);
        let find = |id: &str| lookup.detail.iter().find(|r| r.key == id).expect("value row");
        assert_eq!(find("a1").status, "different");
        assert_eq!(find("a1").source.as_deref(), Some("High"));
        assert_eq!(find("a1").target.as_deref(), Some("Critical"));
        assert_eq!(find("a2").status, "missingTarget");
        assert_eq!(find("a3").status, "missingSource");
    }

    #[test]
    fn identical_lookups_report_same_with_no_detail() {
        let rows = [row("a1", "High", Some("x"))];
        let source = snapshot("dev", &[("CasePriority", "Case priority", &rows)]);
        let target = snapshot("pre", &[("CasePriority", "Case priority", &rows)]);
        let report = diff_snapshots(&source, &target);
        assert_eq!(report.rows[0].status, "same");
        assert!(report.rows[0].detail.is_empty());
        assert!(report.counts.is_empty());
    }

    #[test]
    fn a_lookup_present_on_one_side_only_is_flagged() {
        let source = snapshot("dev", &[("UsrOnlyDev", "Only dev", &[row("a1", "X", None)])]);
        let target = snapshot("pre", &[]);
        let report = diff_snapshots(&source, &target);
        assert_eq!(report.rows[0].status, "missingTarget");
        assert_eq!(report.rows[0].source.as_deref(), Some("1 values"));
        assert_eq!(report.rows[0].target, None);
    }

    #[test]
    fn delete_sql_lists_ids_and_guards_unsafe_tables() {
        assert_eq!(
            build_delete_sql("UsrFlag", &["a1", "a2"]),
            "DELETE FROM \"UsrFlag\" WHERE \"Id\" IN ('a1', 'a2');"
        );
        assert!(build_delete_sql("UsrFlag", &[]).is_empty());
        assert!(build_delete_sql("Bad Table", &["a1"]).is_empty());
        // Ids are escaped like any other literal.
        assert_eq!(
            build_delete_sql("T", &["a'b"]),
            "DELETE FROM \"T\" WHERE \"Id\" IN ('a''b');"
        );
    }

    #[test]
    fn forward_sql_upserts_each_planned_table() {
        let plans = vec![
            TablePlan {
                info: LookupInfo { name: "Case priority".into(), table: "CasePriority".into(), package: "Base".into(), has_description: true },
                source_rows: vec![row("a1", "High", Some("Urgent"))],
            },
            TablePlan {
                info: LookupInfo { name: "Flag".into(), table: "UsrFlag".into(), package: "Custom".into(), has_description: false },
                source_rows: vec![row("b1", "Yes", None)],
            },
        ];
        let sql = forward_sql(&plans);
        assert!(sql.contains("-- Case priority (1 rows)"));
        assert!(sql.contains("INSERT INTO \"CasePriority\""));
        assert!(sql.contains("-- Flag (1 rows)"));
        assert!(sql.contains("INSERT INTO \"UsrFlag\""));
    }

    #[test]
    fn description_only_change_still_reads_as_different() {
        let source = snapshot("dev", &[("T", "T", &[row("a1", "Same", Some("old"))])]);
        let target = snapshot("pre", &[("T", "T", &[row("a1", "Same", Some("new"))])]);
        let report = diff_snapshots(&source, &target);
        assert_eq!(report.rows[0].status, "different");
        assert_eq!(report.rows[0].detail.len(), 1);
        assert_eq!(report.rows[0].detail[0].status, "different");
    }
}
