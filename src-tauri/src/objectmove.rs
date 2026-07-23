//! General object (entity) inspection for data migration — the read side.
//!
//! Where `refdata.rs` handles *lookups* (leaf reference data, three known
//! columns, no inter-table order), this module inspects **arbitrary entity
//! schemas** (Lead, Contact, Account, …) so a later step can copy their full
//! rows between environments. Stage 1 is read-only and answers three questions
//! the Migration UI needs:
//!
//!   * which objects exist (searchable, so the user picks `Lead` not a lookup),
//!   * what columns an object's table has, and
//!   * what other tables it depends on via foreign keys (the hierarchy view).
//!
//! Everything is a plain `SELECT` through the same `refdata::run_select` path as
//! the lookup code, against Creatio's PostgreSQL. Dependencies are read from the
//! database's own FK constraints (`information_schema`), which is exactly the
//! order a raw-SQL insert has to respect.

use crate::jobs::JobState;
use crate::refdata::{escape_literal, is_safe_identifier, migrations_dir, run_select, temp_path};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

/// One migratable entity object: its backing table and owning package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectInfo {
    pub table: String,
    pub package: String,
}

/// One physical column of an object's table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// A foreign-key edge: `column` on this table points at `references_table`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDependency {
    pub column: String,
    pub references_table: String,
}

// -------------------------------------------------------------- pure: SQL

/// Search entity schemas by name (case-insensitive substring), capped so a broad
/// term can't return the whole schema. Only schemas whose physical table exists
/// and carries an `Id` column are listed — the same guard the lookup code uses,
/// which drops abstract/virtual schemas that have no table to copy.
pub fn list_objects_sql(filter: &str) -> String {
    let needle = escape_literal(filter);
    format!(
        r#"SELECT s."Name" AS "Table", COALESCE(p."Name", '') AS "Package"
FROM "SysSchema" s
LEFT JOIN "SysPackage" p ON p."Id" = s."SysPackageId"
WHERE s."ManagerName" = 'EntitySchemaManager'
  AND EXISTS (
    SELECT 1 FROM information_schema.columns c
    WHERE c.table_name = s."Name" AND c.column_name = 'Id'
  )
  AND s."Name" ILIKE '%{needle}%'
ORDER BY s."Name"
LIMIT 300"#
    )
}

/// Every physical column of a table, in definition order.
pub fn columns_sql(table: &str) -> String {
    format!(
        r#"SELECT column_name AS "Column", data_type AS "Type", is_nullable AS "Nullable"
FROM information_schema.columns
WHERE table_name = '{table}'
ORDER BY ordinal_position"#,
        table = escape_literal(table)
    )
}

/// The foreign keys defined on a table (column → referenced table), read from
/// the database's own constraints. Creatio maintains real FK constraints on
/// PostgreSQL, so this is the authoritative dependency order.
pub fn dependencies_sql(table: &str) -> String {
    format!(
        r#"SELECT kcu.column_name AS "Column", ccu.table_name AS "RefTable"
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
  ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
JOIN information_schema.constraint_column_usage ccu
  ON tc.constraint_name = ccu.constraint_name AND tc.table_schema = ccu.table_schema
WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_name = '{table}'
ORDER BY kcu.column_name"#,
        table = escape_literal(table)
    )
}

fn column_index(columns: &[String], name: &str) -> Option<usize> {
    columns.iter().position(|column| column == name)
}

// ------------------------------------------------------------- commands

/// Search entity objects in `env` for the Migration picker. An empty filter is
/// allowed but still capped by the query's `LIMIT`.
// `(async)` runs these clio-backed reads off the UI thread — see sql::run_sql.
#[tauri::command(async)]
pub fn list_objects(env: String, filter: String) -> Result<Vec<ObjectInfo>, String> {
    let (columns, data) = run_select(env.trim(), &list_objects_sql(filter.trim()))?;
    let (Some(table_at), Some(package_at)) =
        (column_index(&columns, "Table"), column_index(&columns, "Package"))
    else {
        return Err("The object search returned an unexpected shape.".to_string());
    };
    Ok(data
        .into_iter()
        .filter_map(|record| {
            let table = record.get(table_at).cloned().unwrap_or_default();
            if table.is_empty() {
                return None;
            }
            let package = record.get(package_at).cloned().unwrap_or_default();
            Some(ObjectInfo { table, package })
        })
        .collect())
}

/// List an object table's columns.
#[tauri::command(async)]
pub fn object_columns(env: String, table: String) -> Result<Vec<ObjectColumn>, String> {
    if !is_safe_identifier(table.trim()) {
        return Err(format!("Refusing an unsafe table name: {table}"));
    }
    let (columns, data) = run_select(env.trim(), &columns_sql(table.trim()))?;
    let (Some(name_at), Some(type_at), Some(nullable_at)) = (
        column_index(&columns, "Column"),
        column_index(&columns, "Type"),
        column_index(&columns, "Nullable"),
    ) else {
        return Err("The column query returned an unexpected shape.".to_string());
    };
    Ok(data
        .into_iter()
        .filter_map(|record| {
            let name = record.get(name_at).cloned().unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            let data_type = record.get(type_at).cloned().unwrap_or_default();
            let nullable =
                record.get(nullable_at).map(|value| value.eq_ignore_ascii_case("YES")).unwrap_or(false);
            Some(ObjectColumn { name, data_type, nullable })
        })
        .collect())
}

/// The foreign-key dependencies of an object table — the input to the hierarchy
/// view and, later, to insert ordering.
#[tauri::command(async)]
pub fn object_dependencies(env: String, table: String) -> Result<Vec<ObjectDependency>, String> {
    if !is_safe_identifier(table.trim()) {
        return Err(format!("Refusing an unsafe table name: {table}"));
    }
    let (columns, data) = run_select(env.trim(), &dependencies_sql(table.trim()))?;
    let (Some(column_at), Some(ref_at)) =
        (column_index(&columns, "Column"), column_index(&columns, "RefTable"))
    else {
        return Err("The dependency query returned an unexpected shape.".to_string());
    };
    Ok(data
        .into_iter()
        .filter_map(|record| {
            let column = record.get(column_at).cloned().unwrap_or_default();
            let references_table = record.get(ref_at).cloned().unwrap_or_default();
            if column.is_empty() || references_table.is_empty() {
                return None;
            }
            Some(ObjectDependency { column, references_table })
        })
        // A table's own PK self-reference and duplicate composite entries add noise.
        .filter(|dep| dep.references_table != table.trim())
        .collect())
}

/// Count the rows in an object table — used for the two-sided source/target
/// refresh view.
#[tauri::command(async)]
pub fn object_row_count(env: String, table: String) -> Result<i64, String> {
    if !is_safe_identifier(table.trim()) {
        return Err(format!("Refusing an unsafe table name: {table}"));
    }
    let sql = format!("SELECT COUNT(*)::text AS \"Count\" FROM \"{}\"", table.trim());
    let (columns, data) = run_select(env.trim(), &sql)?;
    let count_at = column_index(&columns, "Count").ok_or("No count returned.")?;
    let count = data
        .first()
        .and_then(|record| record.get(count_at))
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0);
    Ok(count)
}

// ---------------------------------------------------- pure: full-row copy

/// The lookup/user table Creatio's ownership and audit columns point at. Values
/// in `SysAdminUnit`-referencing columns are per-environment users, so a copy
/// remaps them rather than carrying ids that won't exist on the target.
const OWNER_REF_TABLE: &str = "SysAdminUnit";

/// Column names reduced to bare identifiers (all stock Creatio columns qualify);
/// the guard is absolute so a name can never carry SQL.
fn safe_column_names(columns: &[ObjectColumn]) -> Vec<String> {
    columns.iter().map(|c| c.name.clone()).filter(|name| is_safe_identifier(name)).collect()
}

/// `SELECT` every column cast to text, so uuid/timestamp/numeric values come
/// back in a form that re-inserts cleanly. Optionally filtered to a set of ids
/// (used to read the target's current state for a rollback).
pub fn select_rows_sql(table: &str, columns: &[String], ids: Option<&[String]>) -> String {
    let cols = columns
        .iter()
        .filter(|c| is_safe_identifier(c))
        .map(|c| format!("CAST(\"{c}\" AS text) AS \"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    match ids {
        Some(ids) if !ids.is_empty() => {
            let list =
                ids.iter().map(|id| format!("'{}'", escape_literal(id))).collect::<Vec<_>>().join(", ");
            format!("SELECT {cols} FROM \"{table}\" WHERE \"Id\" IN ({list})")
        }
        _ => format!("SELECT {cols} FROM \"{table}\""),
    }
}

/// Convert a text result into cells aligned to `want`, treating an empty string
/// as SQL NULL. clio exports a NULL and an empty string identically, so this is
/// a deliberate, documented simplification: blank text becomes NULL.
fn cells_from_result(
    want: &[String],
    columns: &[String],
    data: Vec<Vec<String>>,
) -> Vec<Vec<Option<String>>> {
    let index: Vec<Option<usize>> =
        want.iter().map(|w| columns.iter().position(|c| c == w)).collect();
    data.into_iter()
        .map(|record| {
            index
                .iter()
                .map(|at| {
                    at.and_then(|i| record.get(i)).map(|v| v.to_string()).filter(|v| !v.is_empty())
                })
                .collect()
        })
        .collect()
}

/// A full-column idempotent upsert. Keyed on `Id`; every other column is
/// overwritten from the source. Rows align to `columns`.
pub fn build_full_upsert(table: &str, columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    if rows.is_empty() || !is_safe_identifier(table) {
        return String::new();
    }
    let cols: Vec<&String> = columns.iter().filter(|c| is_safe_identifier(c)).collect();
    if cols.is_empty() {
        return String::new();
    }
    let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let values = rows
        .iter()
        .map(|row| {
            let cells = row
                .iter()
                .map(|cell| match cell {
                    Some(v) => format!("'{}'", escape_literal(v)),
                    None => "NULL".to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({cells})")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let set = cols
        .iter()
        .filter(|c| c.as_str() != "Id")
        .map(|c| format!("\"{c}\" = EXCLUDED.\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict = if set.is_empty() {
        "ON CONFLICT (\"Id\") DO NOTHING".to_string()
    } else {
        format!("ON CONFLICT (\"Id\") DO UPDATE SET {set}")
    };
    format!("INSERT INTO \"{table}\" ({collist}) VALUES\n{values}\n{conflict};")
}

/// `DELETE` the given ids — the inverse of an insert, for rollback.
fn build_delete_ids(table: &str, ids: &[String]) -> String {
    if ids.is_empty() || !is_safe_identifier(table) {
        return String::new();
    }
    let list = ids.iter().map(|id| format!("'{}'", escape_literal(id))).collect::<Vec<_>>().join(", ");
    format!("DELETE FROM \"{table}\" WHERE \"Id\" IN ({list});")
}

// ------------------------------------------------- impure: plan & migrate

/// A resolved copy plan: the column order, the source rows (owner already
/// remapped if requested), and each row's Id.
struct ObjectPlan {
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    ids: Vec<String>,
}

/// Columns that reference `SysAdminUnit` (Owner, CreatedBy, ModifiedBy, …).
fn owner_columns(env: &str, table: &str) -> Result<Vec<String>, String> {
    Ok(object_dependencies(env.to_string(), table.to_string())?
        .into_iter()
        .filter(|dep| dep.references_table == OWNER_REF_TABLE)
        .map(|dep| dep.column)
        .collect())
}

/// The Supervisor user's id in `env`, to remap ownership onto.
fn supervisor_id(env: &str) -> Result<String, String> {
    let (columns, data) = run_select(
        env,
        "SELECT CAST(\"Id\" AS text) AS \"Id\" FROM \"SysAdminUnit\" WHERE \"Name\" = 'Supervisor' LIMIT 1",
    )?;
    let at = column_index(&columns, "Id").ok_or("SysAdminUnit returned no Id column.")?;
    data.first()
        .and_then(|record| record.get(at))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("No 'Supervisor' user found in {env} to remap ownership to."))
}

/// Read the source object's rows and build a copy plan, remapping ownership onto
/// the target's Supervisor when asked.
fn plan_object(
    source_env: &str,
    target_env: Option<&str>,
    table: &str,
    remap_owner: bool,
) -> Result<ObjectPlan, String> {
    if !is_safe_identifier(table) {
        return Err(format!("Refusing an unsafe table name: {table}"));
    }
    let columns = safe_column_names(&object_columns(source_env.to_string(), table.to_string())?);
    let Some(id_at) = columns.iter().position(|c| c == "Id") else {
        return Err(format!("{table} has no Id column to key on."));
    };

    let (result_cols, data) = run_select(source_env, &select_rows_sql(table, &columns, None))?;
    let mut rows = cells_from_result(&columns, &result_cols, data);
    let ids: Vec<String> =
        rows.iter().filter_map(|row| row.get(id_at).and_then(|cell| cell.clone())).collect();

    if remap_owner {
        let target = target_env.ok_or("A target is required to remap ownership.")?;
        let owner_cols = owner_columns(source_env, table)?;
        if !owner_cols.is_empty() {
            let supervisor = supervisor_id(target)?;
            let owner_at: Vec<usize> = owner_cols
                .iter()
                .filter_map(|name| columns.iter().position(|c| c == name))
                .collect();
            for row in rows.iter_mut() {
                for &at in &owner_at {
                    if let Some(cell) = row.get_mut(at) {
                        // Only replace a present owner; a NULL stays NULL.
                        if cell.is_some() {
                            *cell = Some(supervisor.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(ObjectPlan { table: table.to_string(), columns, rows, ids })
}

/// A runnable rollback built from the target's current state for the ids the
/// copy will touch: restore rows that already exist, delete rows it will insert.
fn object_rollback_sql(target_env: &str, plan: &ObjectPlan) -> Result<String, String> {
    let (result_cols, data) =
        run_select(target_env, &select_rows_sql(&plan.table, &plan.columns, Some(&plan.ids)))?;
    let current = cells_from_result(&plan.columns, &result_cols, data);
    let id_at = plan.columns.iter().position(|c| c == "Id").unwrap_or(0);
    let present: std::collections::HashSet<String> =
        current.iter().filter_map(|row| row.get(id_at).and_then(|cell| cell.clone())).collect();

    let restore = build_full_upsert(&plan.table, &plan.columns, &current);
    let to_delete: Vec<String> =
        plan.ids.iter().filter(|id| !present.contains(*id)).cloned().collect();
    let delete = build_delete_ids(&plan.table, &to_delete);

    let mut out = format!(
        "-- rollback {} (restore {} / remove {})\n",
        plan.table,
        current.len(),
        to_delete.len()
    );
    if !restore.is_empty() {
        out.push_str(&restore);
        out.push('\n');
    }
    if !delete.is_empty() {
        out.push_str(&delete);
        out.push('\n');
    }
    Ok(out)
}

/// Dry-run: the full-column upsert that would copy the object's rows. Read-only.
#[tauri::command]
pub fn build_object_migration(
    source_env: String,
    target_env: String,
    table: String,
    remap_owner: bool,
) -> Result<String, String> {
    let target = if remap_owner { Some(target_env.trim()) } else { None };
    let plan = plan_object(source_env.trim(), target, table.trim(), remap_owner)?;
    let sql = build_full_upsert(&plan.table, &plan.columns, &plan.rows);
    if sql.is_empty() {
        return Err(format!("{} has no rows to migrate.", table.trim()));
    }
    let header = format!(
        "-- {} ({} rows, {} columns){}\n",
        plan.table,
        plan.rows.len(),
        plan.columns.len(),
        if remap_owner { ", owner → target Supervisor" } else { "" }
    );
    Ok(format!("{header}{sql}\n"))
}

/// Copy one object's rows from `source_env` onto `target_env` as a full-column
/// idempotent upsert. **Mutates the target** — env-locked on both sides, and
/// (unless `skip_backup`) writes a runnable rollback first. Same discipline as
/// `migrate_lookups`; the write goes through `clio_capture` so clio's output can
/// be inspected (execute-sql-script exits 0 even when the database rejects it).
#[tauri::command]
pub fn migrate_object(
    app: AppHandle,
    jobs: State<'_, JobState>,
    source_env: String,
    target_env: String,
    table: String,
    remap_owner: bool,
    skip_backup: bool,
) -> Result<String, String> {
    let source_env = source_env.trim().to_string();
    let target_env = target_env.trim().to_string();
    let table = table.trim().to_string();
    if source_env == target_env {
        return Err("Choose a different target environment.".to_string());
    }
    if !is_safe_identifier(&table) {
        return Err(format!("Refusing an unsafe table name: {table}"));
    }
    let environments = crate::clio::list_environments()?;
    for name in [&source_env, &target_env] {
        if !environments.iter().any(|environment| &environment.name == name) {
            return Err(format!("Environment {name} is not registered in clio."));
        }
    }

    let id = jobs.create_job(
        &app,
        "migrate-object",
        Some(target_env.clone()),
        format!("migrate object {table}: {source_env} → {target_env}"),
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
        if !state.mark_running_phase(&app, &job_id, "reading source rows", true) {
            return;
        }

        let plan = match plan_object(&source_env, Some(&target_env), &table, remap_owner) {
            Ok(plan) => plan,
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
                return;
            }
        };
        let forward = build_full_upsert(&plan.table, &plan.columns, &plan.rows);
        if forward.is_empty() {
            state.log(&app, &job_id, format!("{table} has no rows to migrate."));
            state.finish(&app, &job_id, Some(0));
            return;
        }
        let total_rows = plan.rows.len();

        if !skip_backup {
            if !state.set_phase(&app, &job_id, "backing up target", true) {
                return;
            }
            match object_rollback_sql(&target_env, &plan) {
                Ok(rollback) => {
                    let stamp = crate::jobs::now_ms();
                    match migrations_dir(&app) {
                        Ok(dir) => {
                            let rollback_path =
                                dir.join(format!("rollback-{target_env}-{table}-{stamp}.sql"));
                            let applied_path =
                                dir.join(format!("applied-{target_env}-{table}-{stamp}.sql"));
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

        if !state.set_phase(&app, &job_id, "writing to target", false) {
            return;
        }
        let script = format!("BEGIN;\n{forward}\nCOMMIT;\n");
        let sql_path = temp_path("sql");
        if let Err(error) = std::fs::write(&sql_path, &script) {
            state.log(&app, &job_id, format!("Could not stage the migration: {error}"));
            state.finish(&app, &job_id, Some(1));
            return;
        }
        let sql_str = sql_path.to_string_lossy().to_string();
        state.log(&app, &job_id, format!("Applying {total_rows} row(s) of {table} to {target_env}…"));

        let result = crate::clio::clio_capture(&["execute-sql-script", "-f", &sql_str, "-e", &target_env]);
        let _ = std::fs::remove_file(&sql_path);

        match result {
            Ok((code, out)) => {
                for line in out.lines().filter(|line| !line.trim().is_empty()) {
                    state.log(&app, &job_id, line.to_string());
                }
                if crate::sql::is_failure(code, &out) {
                    state.log(
                        &app,
                        &job_id,
                        format!("Migration failed: {}", crate::sql::friendly_error(&out)),
                    );
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
                        format!("✓ Migrated {total_rows} row(s) of {table} to {target_env}."),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_objects_sql_escapes_the_filter_and_scopes_to_entity_schemas() {
        let sql = list_objects_sql("Lead");
        assert!(sql.contains("ILIKE '%Lead%'"));
        assert!(sql.contains("'EntitySchemaManager'"));
        assert!(sql.contains("LIMIT 300"));
        // A quote in the filter is escaped, never breaking out of the literal.
        assert!(list_objects_sql("O'Brien").contains("ILIKE '%O''Brien%'"));
    }

    #[test]
    fn dependencies_sql_reads_foreign_keys_for_the_table() {
        let sql = dependencies_sql("Lead");
        assert!(sql.contains("FOREIGN KEY"));
        assert!(sql.contains("tc.table_name = 'Lead'"));
    }

    #[test]
    fn columns_sql_orders_by_position() {
        let sql = columns_sql("Contact");
        assert!(sql.contains("table_name = 'Contact'"));
        assert!(sql.contains("ORDER BY ordinal_position"));
    }

    #[test]
    fn select_rows_casts_to_text_and_can_filter_by_ids() {
        let cols = vec!["Id".to_string(), "Name".to_string()];
        let all = select_rows_sql("Lead", &cols, None);
        assert!(all.contains("CAST(\"Id\" AS text) AS \"Id\""));
        assert!(all.ends_with("FROM \"Lead\""));

        let some = select_rows_sql("Lead", &cols, Some(&["a1".to_string(), "a2".to_string()]));
        assert!(some.contains("WHERE \"Id\" IN ('a1', 'a2')"));
        // An empty id set falls back to the whole table rather than a broken IN ().
        assert!(!select_rows_sql("Lead", &cols, Some(&[])).contains("IN ()"));
    }

    #[test]
    fn cells_treat_blank_as_null_and_align_to_wanted_columns() {
        let want = vec!["Id".to_string(), "Name".to_string()];
        // Result columns come back in a different order; alignment must follow names.
        let columns = vec!["Name".to_string(), "Id".to_string()];
        let data = vec![vec!["".to_string(), "a1".to_string()]];
        let cells = cells_from_result(&want, &columns, data);
        assert_eq!(cells, vec![vec![Some("a1".to_string()), None]]);
    }

    #[test]
    fn full_upsert_writes_every_column_and_overwrites_non_id() {
        let columns = vec!["Id".to_string(), "Name".to_string(), "AccountId".to_string()];
        let rows = vec![
            vec![Some("a1".to_string()), Some("Acme".to_string()), None],
            vec![Some("a2".to_string()), Some("O'Neil".to_string()), Some("c9".to_string())],
        ];
        let sql = build_full_upsert("Contact", &columns, &rows);
        assert!(sql.starts_with("INSERT INTO \"Contact\" (\"Id\", \"Name\", \"AccountId\") VALUES"));
        assert!(sql.contains("('a1', 'Acme', NULL)"));
        // Quotes in values are escaped.
        assert!(sql.contains("('a2', 'O''Neil', 'c9')"));
        // Id is the conflict key and is never in the update set.
        assert!(sql.contains("ON CONFLICT (\"Id\") DO UPDATE SET"));
        assert!(sql.contains("\"Name\" = EXCLUDED.\"Name\""));
        assert!(sql.contains("\"AccountId\" = EXCLUDED.\"AccountId\""));
        assert!(!sql.contains("\"Id\" = EXCLUDED.\"Id\""));
    }

    #[test]
    fn full_upsert_of_id_only_table_does_nothing_on_conflict() {
        let columns = vec!["Id".to_string()];
        let rows = vec![vec![Some("a1".to_string())]];
        let sql = build_full_upsert("T", &columns, &rows);
        assert!(sql.contains("ON CONFLICT (\"Id\") DO NOTHING"));
    }

    #[test]
    fn full_upsert_of_nothing_or_unsafe_table_is_empty() {
        assert!(build_full_upsert("Contact", &["Id".to_string()], &[]).is_empty());
        assert!(build_full_upsert(
            "Bad Table",
            &["Id".to_string()],
            &[vec![Some("a1".to_string())]]
        )
        .is_empty());
    }

    #[test]
    fn delete_ids_lists_and_guards_unsafe_tables() {
        assert_eq!(
            build_delete_ids("Contact", &["a1".to_string(), "a2".to_string()]),
            "DELETE FROM \"Contact\" WHERE \"Id\" IN ('a1', 'a2');"
        );
        assert!(build_delete_ids("Contact", &[]).is_empty());
        assert!(build_delete_ids("Bad Table", &["a1".to_string()]).is_empty());
    }
}
