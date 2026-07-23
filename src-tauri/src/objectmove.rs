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

use crate::refdata::{escape_literal, is_safe_identifier, run_select};
use serde::{Deserialize, Serialize};

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
}
