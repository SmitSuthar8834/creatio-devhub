//! Run SQL against a Creatio environment and export the result.
//!
//! Everything goes through `clio execute-sql-script`, which runs the query via
//! the cliogate package and can emit the result as a table, CSV, or XLSX file.
//! For the in-app grid we write a temporary semicolon-delimited CSV and parse it
//! back; for export we let clio write straight to the file the user picked.

use crate::clio;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Max rows returned to the UI grid. Export is unbounded (clio writes the file).
const DISPLAY_CAP: usize = 2000;

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

/// Pull a readable error out of clio's output. Recognizes the cliogate hint and
/// [ERR] lines; otherwise returns the trimmed output.
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
    let trimmed = out.trim();
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
    if code != 0 || out.contains("[ERR]") || !Path::new(&csv_path).exists() {
        let _ = std::fs::remove_file(&csv_path);
        return Err(friendly_error(&out));
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
    if code != 0 || out.contains("[ERR]") || !Path::new(path.trim()).exists() {
        return Err(friendly_error(&out));
    }
    Ok(())
}
