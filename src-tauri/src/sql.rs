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
    /// The SQL was a statement (UPDATE/INSERT/DDL) rather than a query, so
    /// having no rows is the expected outcome and not an empty answer.
    pub statement: bool,
    /// Human-readable notes to show alongside (or instead of) the grid — used
    /// when a run produced no rows but there is something the user should know,
    /// e.g. that PL/pgSQL `RAISE NOTICE` output is not returned by clio.
    pub messages: Vec<String>,
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
/// Neither the exit code nor an `[ERR]` line is enough. When the *database*
/// rejects the SQL, clio exits 0, prints no `[ERR]`, and simply writes the
/// engine's error followed by `Done` — verified against clio 8.1.x. So the
/// output has to be read for the error itself.
///
/// A *missing result file* is deliberately not a failure signal: statements
/// without a result set never produce one, and neither does a query that
/// matched nothing.
pub(crate) fn is_failure(code: i32, out: &str) -> bool {
    code != 0 || out.contains("[ERR]") || database_error(out).is_some()
}

/// Phrases that only appear when an engine rejected the statement. Used for
/// engines that do not print a SQLSTATE the way PostgreSQL does.
const DB_ERROR_PHRASES: &[&str] = &[
    "syntax error",
    "does not exist",
    "already exists",
    "invalid column name",
    "invalid object name",
    "permission denied",
    "duplicate key",
    "violates",
];

/// A PostgreSQL SQLSTATE header such as `42703: column "x" does not exist`.
/// Five alphanumerics, a colon, then a message.
fn is_sqlstate_line(line: &str) -> bool {
    let Some((code, message)) = line.split_once(':') else {
        return false;
    };
    code.len() == 5
        && code.chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        && code.chars().any(|c| c.is_ascii_digit())
        && !message.trim().is_empty()
}

/// The database's own complaint, pulled out of clio's output.
///
/// Returns everything from the error line up to clio's trailing `Done`, so the
/// `POSITION:` hint travels with the message that needs it.
fn database_error(out: &str) -> Option<String> {
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let start = lines.iter().position(|line| {
        is_sqlstate_line(line) || {
            let lowered = line.to_lowercase();
            DB_ERROR_PHRASES.iter().any(|phrase| lowered.contains(phrase))
        }
    })?;
    let message: Vec<&str> = lines[start..]
        .iter()
        .take_while(|line| !line.eq_ignore_ascii_case("done"))
        .filter(|line| !line.is_empty())
        .copied()
        .collect();
    Some(message.join(" "))
}

/// Whether this SQL asks the database for rows back.
///
/// clio writes no result file for a statement *and* for a query that matched
/// nothing, so the output cannot tell the two apart — only the SQL can. Getting
/// this wrong just picks the wrong success wording, never a wrong outcome.
fn returns_rows(query: &str) -> bool {
    let head = query.trim_start().trim_start_matches(['(', '"']);
    let first: String = head
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase();
    matches!(first.as_str(), "select" | "with" | "show" | "explain" | "values" | "table")
}

/// Whether the SQL's only output would be PL/pgSQL messages — an anonymous
/// `DO` block, or a `RAISE NOTICE`. clio's SQL executor returns result sets,
/// not the connection's notice stream, so these run but produce nothing to
/// show. Detecting it lets the UI explain the empty result instead of a bare
/// "ran successfully". Only consulted when the run produced no rows, so a
/// function that both RAISEs and returns a table is unaffected.
fn emits_only_notices(query: &str) -> bool {
    let head = query.trim_start();
    let starts_do = head
        .get(..2)
        .map(|s| s.eq_ignore_ascii_case("do"))
        .unwrap_or(false)
        && head[2..].starts_with(|c: char| c.is_whitespace() || c == '$');
    starts_do || query.to_uppercase().contains("RAISE NOTICE")
}

/// The note shown when a run's only output would have been PL/pgSQL messages.
const NOTICE_ONLY_HINT: &str = "This block ran, but produced no result set. clio's SQL executor \
    does not return PL/pgSQL messages (RAISE NOTICE / anonymous DO-block output), so the notices \
    are lost before they reach DevHub. To see results in the grid, return them as rows instead — \
    e.g. move the logic into a function that RETURNS TABLE(...) and finish the script with a \
    SELECT from it.";

/// Pull a readable error out of clio's output. Recognizes the cliogate hint and
/// [ERR] lines; otherwise returns the trimmed output with clio's own chatter
/// removed — the version-update warning it prepends to every command is never
/// the reason a query failed, and leading it makes the real message look like
/// an aside.
pub(crate) fn friendly_error(out: &str) -> String {
    if out.contains("cliogate") && out.to_lowercase().contains("install") {
        return "This environment needs the cliogate helper to run SQL. Install it first (clio install-gate), then try again.".to_string();
    }
    // The engine's own message is the most useful thing available, and clio
    // prints the failing statement above it — leading with that would bury it.
    if let Some(message) = database_error(out) {
        return message;
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

    /// Real clio 8.1.x output for a query the database rejected: exit code 0,
    /// no [ERR], the SQLSTATE line, then "Done". Reporting this as success is
    /// what made a broken query look like it had run.
    const REJECTED_QUERY: &str = "[WAR] - clio 8.1.0.87 is available. Run 'dotnet tool update clio -g' to update.\n\
        SELECT s.\"Name\", a.\"Name\" AS app\n\
        FROM \"SysSchema\" s\n\
        \n\
        42703: column p.SysInstalledAppId does not exist\n\
        \n\
        POSITION: 186\n\
        Done\n";

    #[test]
    fn a_rejected_query_is_a_failure_despite_exit_zero_and_no_err_marker() {
        assert!(is_failure(0, REJECTED_QUERY));
    }

    #[test]
    fn the_reported_error_is_the_database_message_not_the_echoed_sql() {
        assert_eq!(
            friendly_error(REJECTED_QUERY),
            "42703: column p.SysInstalledAppId does not exist POSITION: 186",
        );
    }

    #[test]
    fn a_rejected_statement_is_a_failure_too() {
        let out = "UPDATE \"NoSuchTableHere\" SET \"X\" = 1 WHERE 1 = 0;\n\
                   \n\
                   42P01: relation \"NoSuchTableHere\" does not exist\n\
                   \n\
                   POSITION: 8\n\
                   Done\n";
        assert!(is_failure(0, out));
    }

    #[test]
    fn a_successful_statement_is_never_mistaken_for_an_error() {
        // The echoed UPDATE contains none of the error signatures.
        let out = "[WAR] - clio 8.1.0.87 is available.\n\
                   UPDATE \"SysPackage\" SET \"Maintainer\" = 'Customer' WHERE \"Name\" = 'QntCreatioERD';\n\
                   \n\
                   Done\n";
        assert!(!is_failure(0, out));
        assert_eq!(database_error(out), None);
    }

    #[test]
    fn queries_are_told_apart_from_statements() {
        assert!(returns_rows("SELECT 1"));
        assert!(returns_rows("  \n select \"Name\" from \"SysPackage\""));
        assert!(returns_rows("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(returns_rows("(SELECT 1)"));
        assert!(!returns_rows("UPDATE \"SysPackage\" SET \"IsChanged\" = TRUE"));
        assert!(!returns_rows("DO $$ BEGIN END $$;"));
        assert!(!returns_rows("insert into \"X\" values (1)"));
    }

    #[test]
    fn notice_only_blocks_are_recognized() {
        // Anonymous DO blocks and RAISE NOTICE: clio returns no rows for these.
        assert!(emits_only_notices("DO $$ BEGIN RAISE NOTICE 'x'; END $$;"));
        assert!(emits_only_notices("do\n$$ begin raise notice 'x'; end $$;"));
        assert!(emits_only_notices("DO$$BEGIN END$$;"));
        // RAISE NOTICE anywhere in the script counts.
        assert!(emits_only_notices("SELECT 1;\nRAISE NOTICE 'hi';"));
    }

    #[test]
    fn ordinary_sql_is_not_notice_only() {
        assert!(!emits_only_notices("SELECT * FROM \"Contact\""));
        assert!(!emits_only_notices("UPDATE \"Contact\" SET \"Name\" = 'x'"));
        // Starts with "DO" but is not a DO block.
        assert!(!emits_only_notices("DOMAIN something"));
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
    query_env(&env, &query)
}

/// The SQL path other modules reuse — the Applications screen reads descriptor
/// fields clio's own commands do not expose. Same rules as the SQL screen: the
/// environment needs cliogate, and callers that treat SQL as an enhancement
/// should tolerate an `Err` rather than surface it.
pub fn query_env(env: &str, query: &str) -> Result<SqlResult, String> {
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
    // No result file means either a statement that succeeded or a query that
    // matched nothing — clio writes one only when there are rows. Failures were
    // already ruled out above, so this is a success either way; `statement` only
    // decides how the UI words it.
    if !Path::new(&csv_path).exists() {
        let messages = if emits_only_notices(&query) {
            vec![NOTICE_ONLY_HINT.to_string()]
        } else {
            Vec::new()
        };
        return Ok(SqlResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            statement: !returns_rows(&query),
            messages,
        });
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
        statement: false,
        messages: Vec::new(),
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
