//! Run SQL against a Creatio environment and export the result.
//!
//! Everything goes through `clio execute-sql-script`, which runs the query via
//! the cliogate package and can emit the result as a table, CSV, or XLSX file.
//! For the in-app grid we write a temporary semicolon-delimited CSV and parse it
//! back; for export we let clio write straight to the file the user picked.
//! The grid is capped at `DISPLAY_CAP` rows so a huge result set can't freeze the
//! UI — `row_count` still reports the true total and exports are never capped.

use crate::clio;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Max rows returned to the UI grid. Export is unbounded (clio writes the file).
const DISPLAY_CAP: usize = 5000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub truncated: bool,
}

fn temp_path(ext: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("devhub-sql-{n}.{ext}"))
}

/// Whether clio's run actually failed.
///
/// The exit code alone is not enough — clio reports some failures as `[ERR]`
/// lines and still exits 0. A *missing result file* is deliberately not a
/// failure signal: statements without a result set never produce one.
fn is_failure(code: i32, out: &str) -> bool {
    code != 0 || out.contains("[ERR]")
}

/// Pull a readable error out of clio's output. Recognizes the cliogate hint and
/// [ERR] lines; otherwise returns the trimmed output with clio's own chatter
/// removed — the version-update warning it prepends to every command is never
/// the reason a query failed, and leading it makes the real message look like
/// an aside.
fn friendly_error(out: &str) -> String {
    if out.contains("cliogate") && out.to_lowercase().contains("install") {
        return "This environment needs the cliogate helper to run SQL. Install it first (clio install-gate), then try again.".to_string();
    }
    let errs: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("[ERR]"))
        .filter_map(|line| line.split_once("[ERR]").map(|(_, message)| message.trim()))
        .filter(|message| !message.is_empty())
        .collect();
    if !errs.is_empty() {
        return errs.join(" ");
    }
    let signal: Vec<&str> = out
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.trim_start().starts_with("[WAR]"))
        .filter(|line| !line.trim_start().starts_with("[INF]"))
        .filter(|line| !line.trim().is_empty())
        .collect();
    let trimmed = signal.join("\n");
    if trimmed.is_empty() {
        "The SQL command produced no output.".to_string()
    } else {
        trimmed.chars().take(600).collect()
    }
}

fn require_query(query: &str) -> Result<String, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Enter a SQL query to run.".to_string());
    }
    Ok(q.to_string())
}

fn require_env(env: &str) -> Result<(), String> {
    if env.trim().is_empty() {
        return Err("Choose an environment.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_clean_clio_error_messages() {
        let output = "[INF] Starting\n[ERR] First failure\n[ERR] Second failure\n";
        assert_eq!(friendly_error(output), "First failure Second failure");
    }

    #[test]
    fn a_statement_without_a_result_set_is_not_a_failure() {
        // Real output from an UPDATE: the version warning, the echoed statement,
        // and "Done" — no [ERR], exit 0, and clio writes no CSV at all. This used
        // to be reported as an error whose text was clio's own success output.
        let out = "[WAR] - clio 8.1.0.87 is available. Run 'dotnet tool update clio -g' to update.\n\
                   UPDATE \"SysPackage\" SET \"Maintainer\" = 'Customer' WHERE \"Name\" = 'QntCreatioERD';\n\
                   \n\
                   Done\n";
        assert!(!is_failure(0, out));
    }

    #[test]
    fn errors_are_still_failures_whatever_the_exit_code() {
        assert!(is_failure(1, "something went wrong"));
        assert!(is_failure(0, "[ERR] - relation \"foo\" does not exist"));
    }

    #[test]
    fn clio_chatter_is_stripped_from_the_fallback_message() {
        let out = "[WAR] - clio 8.1.0.87 is available. Run 'dotnet tool update clio -g' to update.\n\
                   [INF] - connecting\n\
                   syntax error at or near \"SELCT\"\n";
        assert_eq!(friendly_error(out), "syntax error at or near \"SELCT\"");
    }

    #[test]
    fn parses_semicolon_delimited_results() {
        let path = temp_path("csv");
        std::fs::write(
            &path,
            "Id;Name;Notes\r\n1;Alice;\"Contains; semicolon\"\r\n2;Bob;\r\n",
        )
        .expect("write SQL result fixture");

        let result = parse_csv(&path).expect("parse SQL result fixture");
        let _ = std::fs::remove_file(path);

        assert_eq!(result.columns, vec!["Id", "Name", "Notes"]);
        assert_eq!(
            result.rows,
            vec![
                vec!["1", "Alice", "Contains; semicolon"],
                vec!["2", "Bob", ""],
            ]
        );
        assert_eq!(result.row_count, 2);
        assert!(!result.truncated);
    }
}

/// Run `query` against `env` and return the result set for the grid.
#[tauri::command]
pub fn run_sql(env: String, query: String) -> Result<SqlResult, String> {
    let env = env.trim();
    require_env(env)?;
    let query = require_query(&query)?;

    let sql_path = temp_path("sql");
    let csv_path = temp_path("csv");
    std::fs::write(&sql_path, &query).map_err(|e| format!("Could not stage the query: {e}"))?;

    let sql_str = sql_path.to_string_lossy().to_string();
    let csv_str = csv_path.to_string_lossy().to_string();
    let result = clio::clio_capture(&[
        "execute-sql-script",
        "-f",
        &sql_str,
        "-e",
        env,
        "-v",
        "csv",
        "-d",
        &csv_str,
    ]);
    let _ = std::fs::remove_file(&sql_path);

    let (code, out) = result?;
    if is_failure(code, &out) {
        let _ = std::fs::remove_file(&csv_path);
        return Err(friendly_error(&out));
    }
    // A statement with no result set — UPDATE, INSERT, DDL, an anonymous block —
    // succeeds without clio writing the CSV at all. Treating the missing file as
    // a failure used to surface clio's own success output ("… Done") as the error
    // text, so a working UPDATE looked broken. The UI already renders an empty
    // column list as "This statement returned no result set."
    if !Path::new(&csv_path).exists() {
        return Ok(SqlResult { columns: Vec::new(), rows: Vec::new(), row_count: 0, truncated: false });
    }

    let parsed = parse_csv(&csv_path);
    let _ = std::fs::remove_file(&csv_path);
    parsed
}

fn parse_csv(path: &Path) -> Result<SqlResult, String> {
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

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row_count = 0usize;
    for record in reader.records() {
        let record = record.map_err(|e| format!("Malformed result row: {e}"))?;
        row_count += 1;
        if rows.len() < DISPLAY_CAP {
            rows.push(record.iter().map(|s| s.to_string()).collect());
        }
    }

    Ok(SqlResult {
        columns,
        truncated: row_count > rows.len(),
        row_count,
        rows,
    })
}

/// Run `query` against `env` and let clio write the result straight to `path`
/// in `format` ("csv" or "xlsx"). Export is not row-capped.
#[tauri::command]
pub fn export_sql(env: String, query: String, format: String, path: String) -> Result<(), String> {
    let env = env.trim();
    require_env(env)?;
    let query = require_query(&query)?;
    let format = match format.trim().to_lowercase().as_str() {
        "csv" => "csv",
        "xlsx" | "excel" => "xlsx",
        other => return Err(format!("Unsupported export format: {other}")),
    };
    if path.trim().is_empty() {
        return Err("Choose where to save the file.".to_string());
    }

    let sql_path = temp_path("sql");
    std::fs::write(&sql_path, &query).map_err(|e| format!("Could not stage the query: {e}"))?;
    let sql_str = sql_path.to_string_lossy().to_string();

    let result = clio::clio_capture(&[
        "execute-sql-script",
        "-f",
        &sql_str,
        "-e",
        env,
        "-v",
        format,
        "-d",
        path.trim(),
    ]);
    let _ = std::fs::remove_file(&sql_path);

    let (code, out) = result?;
    if is_failure(code, &out) {
        return Err(friendly_error(&out));
    }
    if !Path::new(path.trim()).exists() {
        return Err(
            "The statement ran, but it produced no result set — there is nothing to export."
                .to_string(),
        );
    }
    Ok(())
}
