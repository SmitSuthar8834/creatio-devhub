//! GUID-preserving marketing-content migration over Creatio OData.
//!
//! Ordinary business rows can be inserted through OData. Campaign flow
//! `SysSchema` rows are protected, so their lossless media payloads are read
//! from OData and inserted on the target through the existing cliogate SQL path.

use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const ZERO_GUID: &str = "00000000-0000-0000-0000-000000000000";
const AUDIT: [&str; 5] = ["CreatedById", "ModifiedById", "CreatedOn", "ModifiedOn", "ProcessListeners"];
const BASE_ENTITIES: [&str; 5] = ["BfEmailTemplate", "DCTemplate", "DCReplica", "Campaign", "BulkEmail"];
const ALL_ENTITIES: [&str; 8] = [
    "BfEmailTemplate", "DCTemplate", "DCReplica", "Campaign", "BulkEmail",
    "CampaignVersion", "CampaignItem", "SysLocalizableValue",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentGap {
    pub entity: String,
    pub source_count: usize,
    pub target_count: usize,
    pub missing_count: usize,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentGapReport {
    pub source_env: String,
    pub target_env: String,
    pub entities: Vec<ContentGap>,
    pub broken_flows: Vec<String>,
    pub local_image_references: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowFailure {
    pub id: String,
    pub name: String,
    pub status: u16,
    pub error: String,
}

/// A field the migration changed on purpose so the insert could succeed —
/// surfaced in the report, never silent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowAdjustment {
    pub id: String,
    pub name: String,
    pub column: String,
    /// "remapped" | "cleared" | "auto-included"
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPick {
    pub id: String,
    pub name: String,
    pub exists_in_target: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FkRule {
    pub column: String,
    pub ref_table: String,
}

pub enum FkDecision {
    Keep,
    /// Replace the value (new id, human detail).
    Remap(String, String),
    /// Drop the field (human detail).
    Clear(String),
    /// Refuse to insert the row (reason).
    Block(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMigrateResult {
    pub entity: String,
    pub source_count: usize,
    pub inserted: usize,
    /// Rows that already existed on the target and were overwritten in place.
    pub updated: usize,
    pub skipped: usize,
    pub not_selected: usize,
    pub failures: Vec<RowFailure>,
    pub adjustments: Vec<RowAdjustment>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMigrateReport {
    pub source_env: String,
    pub target_env: String,
    pub rollback_path: String,
    /// JSON snapshot of the rows overwritten in place, when overwrite was used.
    pub overwrite_backup_path: Option<String>,
    pub entities: Vec<EntityMigrateResult>,
}

/// What a copy should do with one source row.
#[derive(Debug, PartialEq)]
pub enum RowPlan {
    /// Process the row. `update` is true when it already exists on the target.
    Process { update: bool },
    /// Leave the target row untouched.
    Skip,
    /// A missing row the user chose not to include.
    NotSelected,
}

/// Decide the fate of one row. `selected` is `None` when the run has no explicit
/// selection (the default is "every row missing on the target"), or `Some(bool)`
/// for membership in an explicit selection. Existing rows are only processed —
/// as an update — when `overwrite` is on.
pub fn plan_row(exists: bool, selected: Option<bool>, overwrite: bool) -> RowPlan {
    let wanted = selected.unwrap_or(!exists);
    if wanted {
        if exists && !overwrite { RowPlan::Skip } else { RowPlan::Process { update: exists } }
    } else if exists {
        RowPlan::Skip
    } else if selected.is_some() {
        RowPlan::NotSelected
    } else {
        RowPlan::Skip
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowMigrateReport {
    pub source_env: String,
    pub target_env: String,
    pub schemas_inserted: usize,
    pub schemas_skipped: usize,
    pub campaigns_repointed: usize,
    pub metadata_bytes: usize,
    pub rollback_path: String,
    pub entities: Vec<EntityMigrateResult>,
    pub failures: Vec<RowFailure>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawEnvironment {
    uri: String,
    login: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClioSettings {
    environments: BTreeMap<String, RawEnvironment>,
}

struct Connection {
    client: Client,
    uri: String,
    csrf: String,
}

#[derive(Debug)]
struct HttpFailure {
    status: u16,
    message: String,
}

fn clio_settings_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "Windows local application data is unavailable.".to_string())?;
    Ok(PathBuf::from(base).join("creatio").join("clio").join("appsettings.json"))
}

fn load_environment(name: &str) -> Result<RawEnvironment, String> {
    let raw = std::fs::read_to_string(clio_settings_path()?)
        .map_err(|e| format!("Could not read clio environment settings: {e}"))?;
    let settings: ClioSettings = serde_json::from_str(&raw)
        .map_err(|e| format!("Could not understand clio environment settings: {e}"))?;
    settings.environments.get(name).cloned()
        .ok_or_else(|| format!("Environment '{name}' is not registered in clio."))
}

fn response_text(response: Response, operation: &str) -> HttpFailure {
    let status = response.status().as_u16();
    let text = response.text().unwrap_or_default();
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    HttpFailure { status, message: format!("{operation} failed (HTTP {status}): {}", compact.chars().take(350).collect::<String>()) }
}

impl Connection {
    fn connect(name: &str) -> Result<Self, String> {
        let env = load_environment(name)?;
        let uri = env.uri.trim_end_matches('/').to_string();
        let client = Client::builder().cookie_store(true).build()
            .map_err(|e| format!("Could not create the Creatio HTTP client: {e}"))?;
        let response = client.post(format!("{uri}/ServiceModel/AuthService.svc/Login"))
            .header(ACCEPT, "application/json")
            .json(&serde_json::json!({"UserName": env.login, "UserPassword": env.password}))
            .send().map_err(|e| format!("Could not connect to environment '{name}': {e}"))?;
        if !response.status().is_success() {
            return Err(response_text(response, &format!("Login to '{name}'")).message);
        }
        let csrf = response.cookies().find(|cookie| cookie.name() == "BPMCSRF")
            .map(|cookie| cookie.value().to_string())
            .ok_or_else(|| format!("Environment '{name}' did not return an authentication token."))?;
        Ok(Self { client, uri, csrf })
    }

    fn request(&self, method: reqwest::Method, url: String) -> reqwest::blocking::RequestBuilder {
        self.client.request(method, url).header("BPMCSRF", &self.csrf).header(ACCEPT, "application/json")
    }

    fn get_all(&self, entity: &str, select: Option<&str>, filter: Option<&str>) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        let mut skip = 0usize;
        loop {
            let mut params = vec![("$top", "100".to_string()), ("$skip", skip.to_string())];
            if let Some(value) = select { params.push(("$select", value.to_string())); }
            if let Some(value) = filter { params.push(("$filter", value.to_string())); }
            let response = self.request(reqwest::Method::GET, format!("{}/0/odata/{entity}", self.uri))
                .query(&params).send().map_err(|e| format!("OData GET {entity} failed: {e}"))?;
            if !response.status().is_success() { return Err(response_text(response, &format!("OData GET {entity}")).message); }
            let json: Value = response.json().map_err(|e| format!("OData GET {entity} returned invalid JSON: {e}"))?;
            let batch = json.get("value").and_then(Value::as_array).cloned().unwrap_or_default();
            let count = batch.len();
            out.extend(batch);
            if count < 100 { break; }
            skip += 100;
        }
        Ok(out)
    }

    fn post(&self, entity: &str, row: &Value) -> Result<(), HttpFailure> {
        let response = self.request(reqwest::Method::POST, format!("{}/0/odata/{entity}", self.uri))
            .header(CONTENT_TYPE, "application/json;charset=utf-8").json(row).send()
            .map_err(|e| HttpFailure { status: 0, message: format!("OData POST {entity} failed: {e}") })?;
        if response.status().is_success() { Ok(()) } else { Err(response_text(response, &format!("OData POST {entity}"))) }
    }

    fn patch(&self, entity: &str, id: &str, row: &Value) -> Result<(), HttpFailure> {
        let response = self.request(reqwest::Method::PATCH, format!("{}/0/odata/{entity}({id})", self.uri))
            .header(CONTENT_TYPE, "application/json;charset=utf-8").json(row).send()
            .map_err(|e| HttpFailure { status: 0, message: format!("OData PATCH {entity} failed: {e}") })?;
        if response.status().is_success() { Ok(()) } else { Err(response_text(response, &format!("OData PATCH {entity}"))) }
    }

    /// Which of `ids` exist in `table` on this environment (lowercased result).
    /// Ids must be GUID-shaped (callers guard with `is_guid`).
    fn existing_ids(&self, table: &str, ids: &BTreeSet<String>) -> Result<BTreeSet<String>, String> {
        let mut found = BTreeSet::new();
        let list: Vec<&String> = ids.iter().collect();
        for chunk in list.chunks(20) {
            let filter = chunk.iter().map(|value| format!("Id eq {value}")).collect::<Vec<_>>().join(" or ");
            found.extend(self.get_all(table, Some("Id"), Some(&filter))?.iter().map(|row| id(row).to_ascii_lowercase()));
        }
        Ok(found)
    }

    fn name_of(&self, table: &str, row_id: &str) -> Option<String> {
        self.get_all(table, Some("Id,Name"), Some(&format!("Id eq {row_id}"))).ok()?
            .first().and_then(|row| row.get("Name")).and_then(Value::as_str)
            .filter(|value| !value.is_empty()).map(str::to_string)
    }

    /// Target id of the single row in `table` named `name_value`; ambiguous or
    /// absent names return None.
    fn id_by_name(&self, table: &str, name_value: &str) -> Option<String> {
        let escaped = name_value.replace('\'', "''");
        let rows = self.get_all(table, Some("Id,Name"), Some(&format!("Name eq '{escaped}'"))).ok()?;
        if rows.len() == 1 { rows.first().map(id) } else { None }
    }

    fn media(&self, schema_id: &str, property: &str) -> Result<Vec<u8>, String> {
        let response = self.request(reqwest::Method::GET, format!("{}/0/odata/SysSchema({schema_id})/{property}", self.uri))
            .send().map_err(|e| format!("Could not read {property}: {e}"))?;
        if !response.status().is_success() { return Err(response_text(response, &format!("GET {property}")).message); }
        response.bytes().map(|b| b.to_vec()).map_err(|e| format!("Could not read {property}: {e}"))
    }
}

pub fn clean_row(row: &Value, source_supervisor: Option<&str>, target_supervisor: Option<&str>, extra_drop: &[&str]) -> Value {
    let Some(input) = row.as_object() else { return Value::Object(Map::new()); };
    let mut output = Map::new();
    for (key, value) in input {
        if key.contains("@odata") || AUDIT.contains(&key.as_str()) || extra_drop.contains(&key.as_str()) || value.is_object() || value.is_array() { continue; }
        if key != "Id" && value.as_str().is_some_and(|v| v.eq_ignore_ascii_case(ZERO_GUID)) { continue; }
        let remapped = match (value.as_str(), source_supervisor, target_supervisor) {
            (Some(value), Some(source), Some(target)) if value.eq_ignore_ascii_case(source) => Value::String(target.to_string()),
            _ => value.clone(),
        };
        output.insert(key.clone(), remapped);
    }
    Value::Object(output)
}

pub fn dependency_order(selected: &[String]) -> Result<Vec<String>, String> {
    let wanted: HashSet<&str> = selected.iter().map(String::as_str).collect();
    for entity in &wanted {
        if !BASE_ENTITIES.contains(entity) { return Err(format!("Unsupported marketing content entity: {entity}")); }
    }
    Ok(BASE_ENTITIES.iter().filter(|entity| wanted.contains(**entity)).map(|s| s.to_string()).collect())
}

pub fn missing_ids(source: &[Value], target: &[Value]) -> BTreeSet<String> {
    let ids = |rows: &[Value]| rows.iter().filter_map(|r| r.get("Id").and_then(Value::as_str)).map(|s| s.to_ascii_lowercase()).collect::<BTreeSet<_>>();
    let left = ids(source); let right = ids(target);
    left.difference(&right).cloned().collect()
}

fn id(row: &Value) -> String { row.get("Id").and_then(Value::as_str).unwrap_or_default().to_string() }
fn name(row: &Value) -> String { row.get("Name").or_else(|| row.get("DisplayName")).and_then(Value::as_str).unwrap_or_default().to_string() }

fn supervisor_id(conn: &Connection) -> Result<String, String> {
    conn.get_all("Contact", Some("Id"), Some("Name eq 'Supervisor'"))?.first()
        .and_then(|row| row.get("Id")).and_then(Value::as_str).map(str::to_string)
        .ok_or_else(|| "Could not resolve the Supervisor contact.".to_string())
}

fn is_guid(value: &str) -> bool {
    value.len() == 36 && value.chars().enumerate().all(|(index, c)| match index {
        8 | 13 | 18 | 23 => c == '-',
        _ => c.is_ascii_hexdigit(),
    })
}

/// Foreign keys of `tables` read from the target database in one query.
/// Needs cliogate on the target; callers treat an error as "no FK metadata"
/// and fall back to plain inserts.
fn fk_rules_for(target_env: &str, tables: &[String]) -> Result<BTreeMap<String, Vec<FkRule>>, String> {
    let safe: Vec<&String> = tables.iter().filter(|table| crate::refdata::is_safe_identifier(table)).collect();
    if safe.is_empty() { return Ok(BTreeMap::new()); }
    let list = safe.iter().map(|table| format!("'{table}'")).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT cl.relname AS \"TableName\", a.attname AS \"ColumnName\", cf.relname AS \"RefTable\" \
         FROM pg_constraint c \
         JOIN pg_class cl ON cl.oid = c.conrelid \
         JOIN pg_class cf ON cf.oid = c.confrelid \
         JOIN unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord) ON true \
         JOIN pg_attribute a ON a.attrelid = cl.oid AND a.attnum = k.attnum \
         WHERE c.contype = 'f' AND cl.relname IN ({list})"
    );
    let (columns, rows) = crate::refdata::run_select(target_env, &sql)?;
    let index = |name: &str| columns.iter().position(|column| column == name);
    let (Some(at_table), Some(at_column), Some(at_ref)) = (index("TableName"), index("ColumnName"), index("RefTable")) else {
        return Ok(BTreeMap::new());
    };
    let mut map: BTreeMap<String, Vec<FkRule>> = BTreeMap::new();
    for row in rows {
        let (Some(table), Some(column), Some(ref_table)) = (row.get(at_table), row.get(at_column), row.get(at_ref)) else { continue; };
        if !crate::refdata::is_safe_identifier(ref_table) { continue; }
        map.entry(table.clone()).or_default().push(FkRule { column: column.clone(), ref_table: ref_table.clone() });
    }
    Ok(map)
}

/// Apply `decide` to every FK field of a cleaned row body. Returns the
/// adjustments made and, if the row must not be inserted, the reason.
pub fn resolve_row(
    row_id: &str,
    row_name: &str,
    body: &mut Map<String, Value>,
    rules: &[FkRule],
    decide: &mut dyn FnMut(&FkRule, &str) -> FkDecision,
) -> (Vec<RowAdjustment>, Option<String>) {
    let mut adjustments = Vec::new();
    for rule in rules {
        if rule.column == "Id" { continue; }
        let Some(value) = body.get(&rule.column).and_then(Value::as_str).map(str::to_string) else { continue; };
        if !is_guid(&value) { continue; }
        match decide(rule, &value) {
            FkDecision::Keep => {}
            FkDecision::Remap(new_id, detail) => {
                body.insert(rule.column.clone(), Value::String(new_id));
                adjustments.push(RowAdjustment { id: row_id.to_string(), name: row_name.to_string(), column: rule.column.clone(), action: "remapped".to_string(), detail });
            }
            FkDecision::Clear(detail) => {
                body.remove(&rule.column);
                adjustments.push(RowAdjustment { id: row_id.to_string(), name: row_name.to_string(), column: rule.column.clone(), action: "cleared".to_string(), detail });
            }
            FkDecision::Block(reason) => return (adjustments, Some(reason)),
        }
    }
    (adjustments, None)
}

/// Order rows of one entity so self-referencing parents insert before their
/// children (e.g. a campaign pointing at a parent campaign). Cycles fall back
/// to the original order and surface as ordinary insert failures.
pub fn order_rows_parents_first(rows: Vec<Value>, self_columns: &[String]) -> Vec<Value> {
    if self_columns.is_empty() { return rows; }
    let mut remaining = rows;
    let mut out: Vec<Value> = Vec::new();
    loop {
        let waiting: BTreeSet<String> = remaining.iter().map(|row| id(row).to_ascii_lowercase()).collect();
        let (ready, rest): (Vec<Value>, Vec<Value>) = remaining.into_iter().partition(|row| {
            self_columns.iter().all(|column| {
                match row.get(column).and_then(Value::as_str).map(|value| value.to_ascii_lowercase()) {
                    Some(parent) => parent == id(row).to_ascii_lowercase() || !waiting.contains(&parent),
                    None => true,
                }
            })
        });
        let progressed = !ready.is_empty();
        out.extend(ready);
        remaining = rest;
        if remaining.is_empty() { break; }
        if !progressed { out.extend(remaining); break; }
    }
    out
}

/// Expand explicit record selections so parents required by selected rows are
/// copied too (BulkEmail → its Campaign, a campaign → its parent campaign).
/// Returns what was auto-added per entity (lowercased ids).
pub fn close_over_parents(
    order: &[String],
    rules: &BTreeMap<String, Vec<FkRule>>,
    source_rows: &BTreeMap<String, Vec<Value>>,
    target_ids: &BTreeMap<String, BTreeSet<String>>,
    chosen: &mut BTreeMap<String, Option<BTreeSet<String>>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let tables: BTreeSet<&str> = order.iter().map(String::as_str).collect();
    let mut added: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for _ in 0..10 {
        let mut changed = false;
        for entity in order {
            let entity_rules: Vec<&FkRule> = rules.get(entity).into_iter().flatten()
                .filter(|rule| tables.contains(rule.ref_table.as_str())).collect();
            if entity_rules.is_empty() { continue; }
            let mut wanted: Vec<(String, String)> = Vec::new();
            for row in source_rows.get(entity).into_iter().flatten() {
                let row_lower = id(row).to_ascii_lowercase();
                let migrating = !target_ids.get(entity).is_some_and(|ids| ids.contains(&row_lower))
                    && chosen.get(entity).and_then(|c| c.as_ref()).map_or(true, |c| c.contains(&row_lower));
                if !migrating { continue; }
                for rule in &entity_rules {
                    let Some(value) = row.get(&rule.column).and_then(Value::as_str) else { continue; };
                    if !is_guid(value) { continue; }
                    let lower = value.to_ascii_lowercase();
                    if rule.ref_table == *entity && lower == row_lower { continue; }
                    if target_ids.get(&rule.ref_table).is_some_and(|ids| ids.contains(&lower)) { continue; }
                    let in_source = source_rows.get(&rule.ref_table)
                        .is_some_and(|rows| rows.iter().any(|r| id(r).eq_ignore_ascii_case(&lower)));
                    if !in_source { continue; }
                    let selected = chosen.get(&rule.ref_table).and_then(|c| c.as_ref()).map_or(true, |c| c.contains(&lower));
                    if !selected { wanted.push((rule.ref_table.clone(), lower)); }
                }
            }
            for (table, lower) in wanted {
                if let Some(Some(set)) = chosen.get_mut(&table) {
                    if set.insert(lower.clone()) {
                        added.entry(table).or_default().insert(lower);
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }
    added
}

/// Live FK resolution state for one migration run. `exists` is seeded with the
/// target's ids for content tables and fed on demand for external tables;
/// tables whose existence cannot be checked keep their values untouched.
struct Resolver<'a> {
    source: &'a Connection,
    target: &'a Connection,
    rules: BTreeMap<String, Vec<FkRule>>,
    content_tables: BTreeSet<String>,
    exists: RefCell<BTreeMap<String, BTreeSet<String>>>,
    unverifiable: RefCell<BTreeSet<String>>,
    pending: RefCell<BTreeMap<String, BTreeSet<String>>>,
    remap_cache: RefCell<BTreeMap<(String, String), Option<(String, String)>>>,
}

impl Resolver<'_> {
    fn rules(&self, table: &str) -> &[FkRule] {
        self.rules.get(table).map(Vec::as_slice).unwrap_or_default()
    }

    /// Batch-check the target for every external FK value the rows carry, so
    /// `decide` never blocks on a per-row round trip for existence.
    fn prime(&self, entity: &str, rows: &[Value]) {
        for rule in self.rules(entity) {
            if self.content_tables.contains(&rule.ref_table) || self.unverifiable.borrow().contains(&rule.ref_table) { continue; }
            let mut wanted: BTreeSet<String> = BTreeSet::new();
            {
                let cache = self.exists.borrow();
                let known = cache.get(&rule.ref_table);
                for row in rows {
                    let Some(value) = row.get(&rule.column).and_then(Value::as_str) else { continue; };
                    if !is_guid(value) || value.eq_ignore_ascii_case(ZERO_GUID) { continue; }
                    if !known.is_some_and(|ids| ids.contains(&value.to_ascii_lowercase())) { wanted.insert(value.to_string()); }
                }
            }
            if wanted.is_empty() { continue; }
            match self.target.existing_ids(&rule.ref_table, &wanted) {
                Ok(found) => { self.exists.borrow_mut().entry(rule.ref_table.clone()).or_default().extend(found); }
                Err(_) => { self.unverifiable.borrow_mut().insert(rule.ref_table.clone()); }
            }
        }
    }

    fn note_inserted(&self, table: &str, row_id: &str) {
        self.pending.borrow_mut().entry(table.to_string()).or_default().insert(row_id.to_ascii_lowercase());
    }

    fn decide(&self, rule: &FkRule, value: &str) -> FkDecision {
        let lower = value.to_ascii_lowercase();
        if self.unverifiable.borrow().contains(&rule.ref_table) { return FkDecision::Keep; }
        if self.exists.borrow().get(&rule.ref_table).is_some_and(|ids| ids.contains(&lower)) { return FkDecision::Keep; }
        if self.pending.borrow().get(&rule.ref_table).is_some_and(|ids| ids.contains(&lower)) { return FkDecision::Keep; }
        if self.content_tables.contains(&rule.ref_table) {
            return FkDecision::Block(format!(
                "{} references {} {} which is not on the target — include that record in the migration.",
                rule.column, rule.ref_table, value
            ));
        }
        let key = (rule.ref_table.clone(), lower);
        if let Some(cached) = self.remap_cache.borrow().get(&key) {
            return match cached {
                Some((new_id, detail)) => FkDecision::Remap(new_id.clone(), detail.clone()),
                None => FkDecision::Clear(format!("No matching {} on the target; the field was left empty.", rule.ref_table)),
            };
        }
        let outcome = self.source.name_of(&rule.ref_table, value).and_then(|source_name| {
            self.target.id_by_name(&rule.ref_table, &source_name)
                .map(|new_id| (new_id, format!("Mapped to the target's {} named '{}'.", rule.ref_table, source_name)))
        });
        self.remap_cache.borrow_mut().insert(key, outcome.clone());
        match outcome {
            Some((new_id, detail)) => FkDecision::Remap(new_id, detail),
            None => FkDecision::Clear(format!("No matching {} on the target; the field was left empty.", rule.ref_table)),
        }
    }
}

fn local_image_hits(entity: &str, rows: &[Value]) -> Vec<String> {
    const NEEDLES: [&str; 4] = ["/0/rest/", "imageservice", "sysimage", "getfile"];
    let mut hits = Vec::new();
    for row in rows {
        let found = row.as_object().is_some_and(|map| map.values().filter_map(Value::as_str)
            .any(|text| { let lower = text.to_ascii_lowercase(); NEEDLES.iter().any(|needle| lower.contains(needle)) }));
        if found { hits.push(format!("{entity}: {}", id(row))); }
    }
    hits
}

fn flow_campaigns(conn: &Connection) -> Result<Vec<Value>, String> {
    Ok(conn.get_all("Campaign", Some("Id,Name,CampaignSchemaUId"), None)?.into_iter().filter(|row| {
        row.get("CampaignSchemaUId").and_then(Value::as_str).is_some_and(|uid| !uid.is_empty() && !uid.eq_ignore_ascii_case(ZERO_GUID))
    }).collect())
}

fn flow_rows(conn: &Connection, campaigns: &[Value]) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for campaign in campaigns {
        let Some(uid) = campaign.get("CampaignSchemaUId").and_then(Value::as_str) else { continue; };
        if let Some(schema) = conn.get_all("SysSchema", None, Some(&format!("UId eq {uid}")))?.into_iter().next() { out.push(schema); }
    }
    Ok(out)
}

fn flow_localizable_values(conn: &Connection, schemas: &[Value], select: Option<&str>) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    for schema in schemas {
        let filter = format!("SysSchema/Id eq {}", id(schema));
        rows.extend(conn.get_all("SysLocalizableValue", select, Some(&filter))?);
    }
    Ok(rows)
}

fn analyze(source_env: &str, target_env: &str) -> Result<ContentGapReport, String> {
    if source_env.trim().is_empty() || target_env.trim().is_empty() || source_env == target_env { return Err("Choose two different environments.".to_string()); }
    let source = Connection::connect(source_env)?;
    let target = Connection::connect(target_env)?;
    let mut entities = Vec::new();
    let mut local_images = Vec::new();
    let source_campaigns = flow_campaigns(&source)?;
    let source_schemas = flow_rows(&source, &source_campaigns)?;
    let target_schemas = flow_rows(&target, &source_campaigns)?;
    for entity in ALL_ENTITIES {
        let (source_rows, target_rows) = if entity == "SysLocalizableValue" {
            (flow_localizable_values(&source, &source_schemas, None)?, flow_localizable_values(&target, &target_schemas, Some("Id"))?)
        } else {
            (source.get_all(entity, None, None)?, target.get_all(entity, Some("Id"), None)?)
        };
        let missing = missing_ids(&source_rows, &target_rows).len();
        let mut notes = Vec::new();
        if entity == "CampaignItem" {
            let empty = source_rows.iter().filter(|r| r.get("Name").and_then(Value::as_str).unwrap_or_default().is_empty()).count();
            if empty > 0 { notes.push(format!("{empty} empty-name rows require SQL")); }
        }
        if ["BfEmailTemplate", "DCTemplate", "DCReplica", "BulkEmail"].contains(&entity) { local_images.extend(local_image_hits(entity, &source_rows)); }
        entities.push(ContentGap { entity: entity.to_string(), source_count: source_rows.len(), target_count: target_rows.len(), missing_count: missing, notes });
    }
    let target_schema_uids: BTreeSet<String> = target.get_all("SysSchema", Some("UId"), Some("ManagerName eq 'CampaignSchemaManager'"))?
        .iter().filter_map(|r| r.get("UId").and_then(Value::as_str)).map(|s| s.to_ascii_lowercase()).collect();
    let broken_flows = source_campaigns.iter().filter_map(|campaign| {
        let uid = campaign.get("CampaignSchemaUId")?.as_str()?;
        (!target_schema_uids.contains(&uid.to_ascii_lowercase())).then(|| name(campaign))
    }).collect();
    Ok(ContentGapReport { source_env: source_env.to_string(), target_env: target_env.to_string(), entities, broken_flows, local_image_references: local_images })
}

fn safe_env(value: &str) -> String { value.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect() }
fn timestamp() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() }
fn migration_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| format!("No application data directory: {e}"))?.join("migrations");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create migration directory: {e}"))?; Ok(dir)
}
fn rollback_path(app: &AppHandle, target: &str, suffix: &str) -> Result<PathBuf, String> {
    Ok(migration_dir(app)?.join(format!("rollback-content-{}-{}-{suffix}.sql", safe_env(target), timestamp())))
}
fn sql_escape(value: &str) -> String { value.replace('\'', "''") }
fn sql_guid(value: Option<&str>) -> String { value.filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case(ZERO_GUID)).map(|v| format!("'{}'", sql_escape(v))).unwrap_or_else(|| "NULL".to_string()) }
fn bool_sql(value: Option<&Value>) -> &'static str { if value.and_then(Value::as_bool).unwrap_or(false) { "true" } else { "false" } }
fn hex(bytes: &[u8]) -> String { const DIGITS: &[u8; 16] = b"0123456789abcdef"; let mut out = String::with_capacity(bytes.len()*2); for b in bytes { out.push(DIGITS[(b>>4) as usize] as char); out.push(DIGITS[(b&15) as usize] as char); } out }

/// Render a JSON scalar as a PostgreSQL literal. Objects and arrays never reach
/// here (`clean_row` drops them); anything unexpected becomes NULL. A GUID rides
/// as a quoted string — PostgreSQL casts the unknown-typed literal to uuid.
fn json_to_sql(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => format!("'{}'", sql_escape(text)),
        _ => "NULL".to_string(),
    }
}

/// One `INSERT … ON CONFLICT ("Id")` statement for a cleaned row. `update`
/// overwrites the conflicting row's non-Id columns from the incoming values;
/// otherwise an existing row is left untouched. Writing through SQL bypasses
/// Creatio's app-level field validation (e.g. "Owner field must be filled in"),
/// which is the whole reason this path exists. Column names are guarded so only
/// safe identifiers are emitted.
pub fn upsert_sql(table: &str, row: &Map<String, Value>, update: bool) -> String {
    let cols: Vec<&String> = row.keys().filter(|col| crate::refdata::is_safe_identifier(col)).collect();
    let names = cols.iter().map(|col| format!("\"{col}\"")).collect::<Vec<_>>().join(",");
    let values = cols.iter().map(|col| json_to_sql(&row[*col])).collect::<Vec<_>>().join(",");
    let action = if update {
        let sets = cols.iter().filter(|col| col.as_str() != "Id")
            .map(|col| format!("\"{col}\"=EXCLUDED.\"{col}\"")).collect::<Vec<_>>().join(",");
        if sets.is_empty() { "DO NOTHING".to_string() } else { format!("DO UPDATE SET {sets}") }
    } else {
        "DO NOTHING".to_string()
    };
    format!("INSERT INTO \"{table}\" ({names}) VALUES ({values}) ON CONFLICT (\"Id\") {action};")
}

pub fn flow_insert_sql(schema: &Value, metadata: &[u8], descriptor: &[u8], supervisor: &str, package_id: &str) -> String {
    let get = |key: &str| schema.get(key).and_then(Value::as_str).unwrap_or_default();
    let caption: String = get("Caption").chars().take(250).collect();
    format!("INSERT INTO \"SysSchema\" (\"Id\",\"UId\",\"Name\",\"Caption\",\"ManagerName\",\"SysPackageId\",\"IsLocked\",\"IsChanged\",\"ExtendParent\",\"Description\",\"Checksum\",\"IsNetStandard\",\"CreatedById\",\"ModifiedById\",\"MetaData\",\"Descriptor\",\"MetaDataModifiedOn\",\"StructureModifiedOn\") VALUES ('{}','{}','{}','{}','CampaignSchemaManager','{}',false,true,false,'{}','{}',{},'{}','{}',decode('{}','hex'),decode('{}','hex'),timezone('utc'::text,CURRENT_TIMESTAMP),timezone('utc'::text,CURRENT_TIMESTAMP)) ON CONFLICT (\"Id\") DO NOTHING;",
        sql_escape(get("Id")), sql_escape(get("UId")), sql_escape(get("Name")), sql_escape(&caption), sql_escape(package_id), sql_escape(get("Description")), sql_escape(get("Checksum")),
        schema.get("IsNetStandard").and_then(Value::as_bool).unwrap_or(true), supervisor, supervisor, hex(metadata), hex(descriptor))
}

/// Map each distinct source `SysPackageId` on `schemas` to the target package
/// with the same Name, so a flow lands in the target's own package row instead
/// of tripping the SysSchema→SysPackage foreign key. Packages read over OData
/// (SysPackage is exposed); an id with no name match keeps its source value.
fn resolve_flow_packages(source: &Connection, target: &Connection, schemas: &[Value]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let source_ids: BTreeSet<String> = schemas.iter()
        .filter_map(|schema| schema.get("SysPackageId").and_then(Value::as_str))
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(ZERO_GUID))
        .map(str::to_string).collect();
    for source_id in source_ids {
        let resolved = source.name_of("SysPackage", &source_id)
            .and_then(|name| target.id_by_name("SysPackage", &name))
            .unwrap_or_else(|| source_id.clone());
        map.insert(source_id, resolved);
    }
    map
}

fn run_sql(target: &str, sql_text: &str) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("devhub-content-{}.sql", timestamp()));
    std::fs::write(&path, sql_text).map_err(|e| format!("Could not stage migration SQL: {e}"))?;
    let path_text = path.to_string_lossy().to_string();
    let result = crate::clio::clio_capture(&["execute-sql-script", "-f", &path_text, "-e", target]);
    let _ = std::fs::remove_file(path);
    let (code, output) = result?;
    if crate::sql::is_failure(code, &output) { Err(crate::sql::friendly_error(&output)) } else { Ok(()) }
}

fn write_delete_rollback(path: &Path, batches: &[(String, Vec<String>)]) -> Result<(), String> {
    let mut sql = String::from("BEGIN;\n");
    for (table, ids) in batches.iter().rev() {
        if ids.is_empty() { continue; }
        sql.push_str(&format!("DELETE FROM \"{table}\" WHERE \"Id\" IN ({});\n", ids.iter().map(|id| format!("'{}'", sql_escape(id))).collect::<Vec<_>>().join(",")));
    }
    sql.push_str("COMMIT;\n");
    std::fs::write(path, sql).map_err(|e| format!("Could not write rollback SQL: {e}"))
}

fn append_flow_repoint_rollback(path: &Path, campaigns: &[Value], target_campaigns: &[Value]) -> Result<(), String> {
    let mut sql = std::fs::read_to_string(path).map_err(|e| format!("Could not extend rollback SQL: {e}"))?;
    let marker = "COMMIT;\n";
    let at = sql.rfind(marker).unwrap_or(sql.len());
    let mut updates = String::new();
    let previous: BTreeMap<String, Option<String>> = target_campaigns.iter().map(|row| {
        (id(row).to_ascii_lowercase(), row.get("CampaignSchemaUId").and_then(Value::as_str).filter(|value| !value.eq_ignore_ascii_case(ZERO_GUID)).map(str::to_string))
    }).collect();
    for campaign in campaigns {
        let old = previous.get(&id(campaign).to_ascii_lowercase()).and_then(|value| value.as_deref());
        updates.push_str(&format!("UPDATE \"Campaign\" SET \"CampaignSchemaUId\" = {} WHERE \"Id\" = '{}';\n", sql_guid(old), sql_escape(&id(campaign))));
    }
    sql.insert_str(at, &updates);
    std::fs::write(path, sql).map_err(|e| format!("Could not extend rollback SQL: {e}"))
}

struct CopyPlan<'a> {
    selection: Option<&'a BTreeSet<String>>,
    resolver: Option<&'a Resolver<'a>>,
    /// Overwrite selected rows that already exist on the target.
    overwrite: bool,
    /// When set, only these columns are written (Id is always kept).
    columns: Option<&'a BTreeSet<String>>,
    /// Write through direct SQL upsert instead of OData — bypasses app-level
    /// field validation. Requires cliogate on the target.
    sql: bool,
}

const PLAIN_COPY: CopyPlan<'static> = CopyPlan { selection: None, resolver: None, overwrite: false, columns: None, sql: false };

fn copy_entity(source: &Connection, target: &Connection, entity: &str, target_env: &str, source_supervisor: &str, target_supervisor: &str, extra_drop: &[&str], sql_empty_names: bool, plan: CopyPlan<'_>) -> Result<(EntityMigrateResult, Vec<Value>), String> {
    let source_rows = source.get_all(entity, None, None)?;
    let target_rows = target.get_all(entity, Some("Id"), None)?;
    let target_ids: BTreeSet<String> = target_rows.iter().map(id).map(|value| value.to_ascii_lowercase()).collect();

    let mut todo: Vec<Value> = Vec::new();
    let mut not_selected = 0;
    let mut skipped = 0;
    for row in &source_rows {
        let lower = id(row).to_ascii_lowercase();
        let exists = target_ids.contains(&lower);
        let selected = plan.selection.map(|set| set.contains(&lower));
        match plan_row(exists, selected, plan.overwrite) {
            RowPlan::Process { .. } => todo.push(row.clone()),
            RowPlan::Skip => skipped += 1,
            RowPlan::NotSelected => not_selected += 1,
        }
    }
    if let Some(resolver) = plan.resolver {
        let self_columns: Vec<String> = resolver.rules(entity).iter()
            .filter(|rule| rule.ref_table == entity).map(|rule| rule.column.clone()).collect();
        todo = order_rows_parents_first(todo, &self_columns);
        resolver.prime(entity, &todo);
    }
    let mut inserted = 0; let mut updated = 0; let mut failures = Vec::new(); let mut sql_rows = Vec::new(); let mut adjustments = Vec::new();
    // SQL mode collects one upsert per row and runs them in a single transaction
    // after the loop; OData mode writes each row as it goes.
    let mut sql_batch: Vec<String> = Vec::new();
    let mut sql_pending: Vec<(String, bool)> = Vec::new();
    for row in &todo {
        if sql_empty_names && row.get("Name").and_then(Value::as_str).unwrap_or_default().is_empty() { sql_rows.push(row.clone()); continue; }
        let body = clean_row(row, Some(source_supervisor), Some(target_supervisor), extra_drop);
        let Value::Object(mut map) = body else { continue; };
        if let Some(resolver) = plan.resolver {
            let (mut adjusted, blocked) = resolve_row(&id(row), &name(row), &mut map, resolver.rules(entity), &mut |rule, value| resolver.decide(rule, value));
            adjustments.append(&mut adjusted);
            if let Some(reason) = blocked {
                failures.push(RowFailure { id: id(row), name: name(row), status: 0, error: reason });
                continue;
            }
        }
        // Restrict to the chosen columns; Id is always kept so the row can be keyed.
        if let Some(columns) = plan.columns { map.retain(|key, _| key == "Id" || columns.contains(key)); }
        let exists = target_ids.contains(&id(row).to_ascii_lowercase());
        if plan.sql {
            sql_batch.push(upsert_sql(entity, &map, exists));
            sql_pending.push((id(row), exists));
            continue;
        }
        let outcome = if exists {
            // OData PATCH keys on the URL id, so the primary key must not also
            // ride in the body.
            map.remove("Id");
            target.patch(entity, &id(row), &Value::Object(map))
        } else {
            target.post(entity, &Value::Object(map))
        };
        match outcome {
            Ok(()) => {
                if exists { updated += 1; } else { inserted += 1; }
                if let Some(resolver) = plan.resolver { resolver.note_inserted(entity, &id(row)); }
            }
            Err(error) => failures.push(RowFailure { id: id(row), name: name(row), status: error.status, error: error.message }),
        }
    }
    if plan.sql && !sql_batch.is_empty() {
        let statement = format!("BEGIN;\n{}\nCOMMIT;", sql_batch.join("\n"));
        match run_sql(target_env, &statement) {
            Ok(()) => for (row_id, exists) in &sql_pending {
                if *exists { updated += 1; } else { inserted += 1; }
                if let Some(resolver) = plan.resolver { resolver.note_inserted(entity, row_id); }
            },
            // The whole transaction rolled back, so no row landed; report the
            // engine's message once rather than repeating it per row.
            Err(error) => failures.push(RowFailure { id: String::new(), name: format!("{} rows via SQL", sql_pending.len()), status: 0, error }),
        }
    }
    Ok((EntityMigrateResult { entity: entity.to_string(), source_count: source_rows.len(), inserted, updated, skipped, not_selected, failures, adjustments }, sql_rows))
}

#[tauri::command]
pub fn content_analyze(source_env: String, target_env: String) -> Result<ContentGapReport, String> { analyze(&source_env, &target_env) }

#[tauri::command]
pub fn content_verify(source_env: String, target_env: String) -> Result<ContentGapReport, String> { analyze(&source_env, &target_env) }

#[tauri::command]
pub fn content_list_records(source_env: String, target_env: String, entity: String) -> Result<Vec<RecordPick>, String> {
    if !BASE_ENTITIES.contains(&entity.as_str()) { return Err(format!("Unsupported marketing content entity: {entity}")); }
    if source_env.trim().is_empty() || target_env.trim().is_empty() || source_env == target_env { return Err("Choose two different environments.".to_string()); }
    let source = Connection::connect(&source_env)?;
    let target = Connection::connect(&target_env)?;
    let rows = source.get_all(&entity, Some("Id,Name"), None)
        .or_else(|_| source.get_all(&entity, Some("Id"), None))?;
    let existing: BTreeSet<String> = target.get_all(&entity, Some("Id"), None)?
        .iter().map(id).map(|value| value.to_ascii_lowercase()).collect();
    Ok(rows.iter().map(|row| RecordPick {
        id: id(row),
        name: name(row),
        exists_in_target: existing.contains(&id(row).to_ascii_lowercase()),
    }).collect())
}

/// The writable columns of an entity, drawn from the source rows: every scalar
/// key that survives `clean_row`'s filter (no audit, OData or nested fields).
/// Feeds the column picker so a user can leave a column — e.g. `OwnerId` — out.
#[tauri::command]
pub fn content_list_columns(source_env: String, entity: String) -> Result<Vec<String>, String> {
    if !BASE_ENTITIES.contains(&entity.as_str()) { return Err(format!("Unsupported marketing content entity: {entity}")); }
    if source_env.trim().is_empty() { return Err("Choose a source environment.".to_string()); }
    let source = Connection::connect(&source_env)?;
    let rows = source.get_all(&entity, None, None)?;
    let mut columns: BTreeSet<String> = BTreeSet::new();
    for row in &rows {
        let Value::Object(map) = clean_row(row, None, None, &[]) else { continue; };
        for key in map.keys() { if key != "Id" { columns.insert(key.clone()); } }
    }
    Ok(columns.into_iter().collect())
}

#[tauri::command]
pub fn content_migrate(app: AppHandle, source_env: String, target_env: String, entities: Vec<String>, selections: Option<BTreeMap<String, Vec<String>>>, overwrite: Option<Vec<String>>, columns: Option<BTreeMap<String, Vec<String>>>, sql_write: Option<bool>) -> Result<ContentMigrateReport, String> {
    if source_env == target_env { return Err("Choose two different environments.".to_string()); }
    let order = dependency_order(&entities)?;
    if order.is_empty() { return Err("Select at least one content entity.".to_string()); }
    let overwrite_set: BTreeSet<String> = overwrite.unwrap_or_default().into_iter().collect();
    let sql_write = sql_write.unwrap_or(false);
    let column_sets: BTreeMap<String, BTreeSet<String>> = columns.unwrap_or_default().into_iter()
        .map(|(entity, list)| (entity, list.into_iter().collect())).collect();
    let source = Connection::connect(&source_env)?; let target = Connection::connect(&target_env)?;
    let source_supervisor = supervisor_id(&source)?; let target_supervisor = supervisor_id(&target)?;

    // FK metadata needs SQL (cliogate) on the target. Without it the run
    // degrades to plain inserts — exactly the pre-resolution behavior.
    let rules = fk_rules_for(&target_env, &order).unwrap_or_default();

    let mut source_rows: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut target_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entity in &order {
        source_rows.insert(entity.clone(), source.get_all(entity, None, None)?);
        target_ids.insert(entity.clone(), target.get_all(entity, Some("Id"), None)?
            .iter().map(id).map(|value| value.to_ascii_lowercase()).collect());
    }

    let mut chosen: BTreeMap<String, Option<BTreeSet<String>>> = order.iter().map(|entity| {
        let selection = selections.as_ref().and_then(|map| map.get(entity))
            .map(|ids| ids.iter().map(|value| value.to_ascii_lowercase()).collect());
        (entity.clone(), selection)
    }).collect();
    let auto_included = close_over_parents(&order, &rules, &source_rows, &target_ids, &mut chosen);

    let mut rollback_batches = Vec::new();
    for entity in &order {
        let ids: Vec<String> = source_rows[entity].iter().filter(|row| {
            let lower = id(row).to_ascii_lowercase();
            !target_ids[entity].contains(&lower)
                && chosen[entity].as_ref().map_or(true, |set| set.contains(&lower))
        }).map(id).collect();
        rollback_batches.push((entity.clone(), ids));
    }
    let rollback = rollback_path(&app, &target_env, "rows")?; write_delete_rollback(&rollback, &rollback_batches)?;

    // Rows we are about to overwrite in place cannot be undone by the DELETE
    // rollback (that would drop pre-existing data), so snapshot them first.
    let overwrite_backup_path = write_overwrite_backup(&app, &target, &target_env, &order, &overwrite_set, &chosen, &target_ids)?;

    let resolver = (!rules.is_empty()).then(|| {
        let mut exists: BTreeMap<String, BTreeSet<String>> = target_ids.clone();
        // Content tables referenced by FKs but excluded from this run still need
        // their target ids so an existing parent is kept, not blocked.
        for table in BASE_ENTITIES {
            if !exists.contains_key(table) && rules.values().flatten().any(|rule| rule.ref_table == table) {
                if let Ok(rows) = target.get_all(table, Some("Id"), None) {
                    exists.insert(table.to_string(), rows.iter().map(id).map(|value| value.to_ascii_lowercase()).collect());
                }
            }
        }
        Resolver {
            source: &source,
            target: &target,
            rules: rules.clone(),
            content_tables: BASE_ENTITIES.iter().map(|table| table.to_string()).collect(),
            exists: RefCell::new(exists),
            unverifiable: RefCell::new(BTreeSet::new()),
            pending: RefCell::new(BTreeMap::new()),
            remap_cache: RefCell::new(BTreeMap::new()),
        }
    });

    let mut results = Vec::new();
    for entity in &order {
        let drop = if entity == "Campaign" { &["CampaignSchemaUId"][..] } else { &[][..] };
        let plan = CopyPlan {
            selection: chosen[entity].as_ref(),
            resolver: resolver.as_ref(),
            overwrite: overwrite_set.contains(entity),
            columns: column_sets.get(entity),
            sql: sql_write,
        };
        let (mut result, _) = copy_entity(&source, &target, entity, &target_env, &source_supervisor, &target_supervisor, drop, false, plan)?;
        for lower in auto_included.get(entity).into_iter().flatten() {
            let display = source_rows[entity].iter().find(|row| id(row).eq_ignore_ascii_case(lower)).map(name).unwrap_or_default();
            result.adjustments.push(RowAdjustment { id: lower.clone(), name: display, column: String::new(), action: "auto-included".to_string(), detail: "Copied because selected records reference it.".to_string() });
        }
        results.push(result);
    }
    Ok(ContentMigrateReport { source_env, target_env, rollback_path: rollback.to_string_lossy().to_string(), overwrite_backup_path, entities: results })
}

/// Snapshot every target row about to be overwritten to a JSON file, so an
/// unwanted overwrite can be reviewed and restored by hand. Returns the file
/// path, or `None` when nothing is being overwritten.
fn write_overwrite_backup(
    app: &AppHandle,
    target: &Connection,
    target_env: &str,
    order: &[String],
    overwrite_set: &BTreeSet<String>,
    chosen: &BTreeMap<String, Option<BTreeSet<String>>>,
    target_ids: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<String>, String> {
    let mut backup = Map::new();
    for entity in order {
        if !overwrite_set.contains(entity) { continue; }
        // Only rows explicitly selected AND already present on the target are
        // overwritten; the all-missing default never touches existing rows.
        let Some(chosen_ids) = chosen.get(entity).and_then(|value| value.as_ref()) else { continue; };
        let empty = BTreeSet::new();
        let present = target_ids.get(entity).unwrap_or(&empty);
        let ids: BTreeSet<String> = chosen_ids.iter().filter(|id| present.contains(*id)).cloned().collect();
        if ids.is_empty() { continue; }
        let saved: Vec<Value> = target.get_all(entity, None, None)?
            .into_iter().filter(|row| ids.contains(&id(row).to_ascii_lowercase())).collect();
        if !saved.is_empty() { backup.insert(entity.clone(), Value::Array(saved)); }
    }
    if backup.is_empty() { return Ok(None); }
    let path = migration_dir(app)?.join(format!("overwrite-backup-{}-{}.json", safe_env(target_env), timestamp()));
    let json = serde_json::to_string_pretty(&Value::Object(backup)).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(&path, json).map_err(|e| format!("Could not write overwrite backup: {e}"))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

fn campaign_item_sql(rows: &[Value], supervisor: &str) -> String {
    rows.iter().map(|r| format!("INSERT INTO \"CampaignItem\" (\"Id\",\"CampaignId\",\"Name\",\"IsDeleted\",\"CampaignElementType\",\"RecordId\",\"SysSchemaUId\",\"CreatedById\",\"ModifiedById\") VALUES ('{}',{},'{}',{},'{}',{},{},'{}','{}') ON CONFLICT (\"Id\") DO NOTHING;",
        sql_escape(&id(r)), sql_guid(r.get("CampaignId").and_then(Value::as_str)), sql_escape(r.get("Name").and_then(Value::as_str).unwrap_or_default()), bool_sql(r.get("IsDeleted")), sql_escape(r.get("CampaignElementType").and_then(Value::as_str).unwrap_or_default()), sql_guid(r.get("RecordId").and_then(Value::as_str)), sql_guid(r.get("SysSchemaUId").and_then(Value::as_str)), supervisor, supervisor)).collect::<Vec<_>>().join("\n")
}

#[tauri::command]
pub fn content_migrate_flows(app: AppHandle, source_env: String, target_env: String) -> Result<FlowMigrateReport, String> {
    if source_env == target_env { return Err("Choose two different environments.".to_string()); }
    let source = Connection::connect(&source_env)?; let target = Connection::connect(&target_env)?;
    let source_supervisor = supervisor_id(&source)?; let target_supervisor = supervisor_id(&target)?;
    let campaigns = flow_campaigns(&source)?; let schemas = flow_rows(&source, &campaigns)?;
    let target_campaigns = target.get_all("Campaign", Some("Id,CampaignSchemaUId"), None)?;
    let existing: BTreeSet<String> = target.get_all("SysSchema", Some("Id"), None)?.iter().map(id).map(|s| s.to_ascii_lowercase()).collect();
    let missing_schemas: Vec<&Value> = schemas.iter().filter(|s| !existing.contains(&id(s).to_ascii_lowercase())).collect();
    let rollback = rollback_path(&app, &target_env, "flows")?;
    let mut batches = vec![("SysSchema".to_string(), missing_schemas.iter().map(|s| id(s)).collect())];
    for entity in ["CampaignVersion", "CampaignItem"] {
        let src = source.get_all(entity, None, None)?; let tgt = target.get_all(entity, Some("Id"), None)?; let missing = missing_ids(&src, &tgt);
        batches.push((entity.to_string(), src.iter().filter(|r| missing.contains(&id(r).to_ascii_lowercase())).map(id).collect()));
    }
    let mut rollback_slv = Vec::new();
    for schema in &schemas {
        let schema_id = id(schema);
        let filter = format!("SysSchema/Id eq {schema_id}");
        let src = source.get_all("SysLocalizableValue", None, Some(&filter))?;
        let tgt = target.get_all("SysLocalizableValue", Some("Id"), Some(&filter))?;
        let missing = missing_ids(&src, &tgt);
        rollback_slv.extend(src.iter().filter(|r| missing.contains(&id(r).to_ascii_lowercase())).map(id));
    }
    batches.push(("SysLocalizableValue".to_string(), rollback_slv));
    write_delete_rollback(&rollback, &batches)?;
    append_flow_repoint_rollback(&rollback, &campaigns, &target_campaigns)?;
    // A flow's SysSchema points at its package (SysPackageId), whose id differs
    // per environment — the real cause of the 23503 foreign-key failure. Map it
    // to the target's package of the same name rather than disabling the FK's
    // system trigger, which needs superuser and is refused (42501) for the
    // ordinary Creatio database account.
    let packages = resolve_flow_packages(&source, &target, &schemas);
    let mut sql = String::from("BEGIN;\n"); let mut metadata_bytes = 0;
    for schema in &missing_schemas {
        let metadata = source.media(&id(schema), "MetaData")?; let descriptor = source.media(&id(schema), "Descriptor")?;
        metadata_bytes += metadata.len();
        let source_pkg = schema.get("SysPackageId").and_then(Value::as_str).unwrap_or_default();
        let package_id = packages.get(source_pkg).map(String::as_str).unwrap_or(source_pkg);
        sql.push_str(&flow_insert_sql(schema, &metadata, &descriptor, &target_supervisor, package_id)); sql.push('\n');
    }
    sql.push_str("COMMIT;\n"); if !missing_schemas.is_empty() { run_sql(&target_env, &sql)?; }
    for schema in &missing_schemas {
        let source_len = source.media(&id(schema), "MetaData")?.len();
        let target_len = target.media(&id(schema), "MetaData")?.len();
        if source_len != target_len { return Err(format!("Flow {} MetaData length differs after insert ({source_len} vs {target_len} bytes).", id(schema))); }
    }
    let mut failures = Vec::new(); let mut repointed = 0;
    for campaign in &campaigns { let campaign_id = id(campaign); let uid = campaign.get("CampaignSchemaUId").and_then(Value::as_str).unwrap_or_default(); match target.patch("Campaign", &campaign_id, &serde_json::json!({"CampaignSchemaUId": uid})) { Ok(()) => repointed += 1, Err(e) => failures.push(RowFailure { id: campaign_id, name: name(campaign), status: e.status, error: e.message }) } }
    let mut results = Vec::new();
    let (versions, _) = copy_entity(&source, &target, "CampaignVersion", &target_env, &source_supervisor, &target_supervisor, &[], false, PLAIN_COPY)?; results.push(versions);
    let (mut items, sql_items) = copy_entity(&source, &target, "CampaignItem", &target_env, &source_supervisor, &target_supervisor, &[], true, PLAIN_COPY)?;
    if !sql_items.is_empty() { run_sql(&target_env, &format!("BEGIN;\n{}\nCOMMIT;", campaign_item_sql(&sql_items, &target_supervisor)))?; items.inserted += sql_items.len(); }
    results.push(items);
    // Captions are scoped strictly to the migrated flow schema rows.
    let slv_source = flow_localizable_values(&source, &schemas, None)?;
    let slv_target = flow_localizable_values(&target, &schemas, Some("Id"))?;
    let slv_missing = missing_ids(&slv_source, &slv_target); let mut slv_result = EntityMigrateResult { entity: "SysLocalizableValue".to_string(), source_count: slv_source.len(), inserted: 0, updated: 0, skipped: slv_source.len() - slv_missing.len(), not_selected: 0, failures: Vec::new(), adjustments: Vec::new() };
    for row in slv_source.iter().filter(|r| slv_missing.contains(&id(r).to_ascii_lowercase())) { let body = clean_row(row, Some(&source_supervisor), Some(&target_supervisor), &[]); match target.post("SysLocalizableValue", &body) { Ok(()) => slv_result.inserted += 1, Err(e) => slv_result.failures.push(RowFailure { id: id(row), name: name(row), status: e.status, error: e.message }) } }
    results.push(slv_result);
    Ok(FlowMigrateReport { source_env, target_env, schemas_inserted: missing_schemas.len(), schemas_skipped: schemas.len()-missing_schemas.len(), campaigns_repointed: repointed, metadata_bytes, rollback_path: rollback.to_string_lossy().to_string(), entities: results, failures })
}

#[tauri::command]
pub fn content_finalize(target_env: String) -> Result<(), String> {
    if target_env.trim().is_empty() { return Err("Choose a target environment.".to_string()); }
    for command in ["clear-redis-db", "restart-web-app"] { let (code, output) = crate::clio::clio_capture(&[command, "-e", &target_env])?; if code != 0 || crate::diagnostics::failure_despite_zero_exit(&output.lines().map(str::to_string).collect::<Vec<_>>()).is_some() { return Err(format!("{command} failed: {}", output.lines().take(8).collect::<Vec<_>>().join(" "))); } }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn cleaning_drops_audit_zero_annotations_objects_and_remaps() { let row = serde_json::json!({"Id":"1","OwnerId":"SRC","CreatedOn":"x","EmptyId":ZERO_GUID,"x@odata.mediaReadLink":"x","Nav":{"Id":"2"},"Name":"ok"}); let clean = clean_row(&row, Some("src"), Some("TARGET"), &[]); assert_eq!(clean, serde_json::json!({"Id":"1","OwnerId":"TARGET","Name":"ok"})); }
    #[test] fn campaign_extra_column_is_dropped() { let clean = clean_row(&serde_json::json!({"Id":"1","CampaignSchemaUId":"u"}), None, None, &["CampaignSchemaUId"]); assert!(clean.get("CampaignSchemaUId").is_none()); }
    #[test] fn dependency_order_is_canonical() { let got = dependency_order(&["BulkEmail".into(), "Campaign".into(), "BfEmailTemplate".into()]).unwrap(); assert_eq!(got, ["BfEmailTemplate","Campaign","BulkEmail"]); }
    #[test] fn dependency_order_rejects_unknown() { assert!(dependency_order(&["SysSchema".into()]).is_err()); }
    #[test] fn gap_diff_is_case_insensitive() { let a=vec![serde_json::json!({"Id":"ABC"}),serde_json::json!({"Id":"DEF"})]; let b=vec![serde_json::json!({"Id":"abc"})]; assert_eq!(missing_ids(&a,&b), BTreeSet::from(["def".to_string()])); }
    #[test] fn hex_is_exact() { assert_eq!(hex(&[0, 1, 15, 16, 255]), "00010f10ff"); }
    #[test] fn flow_sql_escapes_quotes_unicode_and_hex() { let s=serde_json::json!({"Id":"i","UId":"u","Name":"O'Brien — flow","Caption":"It's — live","SysPackageId":"p","Description":"d's","Checksum":"c","IsNetStandard":true}); let sql=flow_insert_sql(&s,b"{}",&[0xff],"sup","target-pkg"); assert!(sql.contains("O''Brien — flow")); assert!(sql.contains("It''s — live")); assert!(sql.contains("'target-pkg'")); assert!(!sql.contains(",'p',")); assert!(sql.contains("decode('7b7d','hex')")); assert!(sql.contains("decode('ff','hex')")); }
    #[test]
    fn json_to_sql_renders_scalars_and_quotes_strings() {
        assert_eq!(json_to_sql(&Value::Null), "NULL");
        assert_eq!(json_to_sql(&serde_json::json!(true)), "true");
        assert_eq!(json_to_sql(&serde_json::json!(42)), "42");
        assert_eq!(json_to_sql(&serde_json::json!("it's")), "'it''s'");
        // Objects never reach here, but if one did it is neutralized, not injected.
        assert_eq!(json_to_sql(&serde_json::json!({"x":1})), "NULL");
    }
    #[test]
    fn upsert_sql_updates_or_leaves_and_guards_columns() {
        let mut row = Map::new();
        row.insert("Id".into(), serde_json::json!("g"));
        row.insert("Name".into(), serde_json::json!("N"));
        row.insert("OwnerId".into(), Value::Null);
        row.insert("bad; drop".into(), serde_json::json!("x")); // unsafe identifier dropped
        let update = upsert_sql("Campaign", &row, true);
        assert!(update.contains("INSERT INTO \"Campaign\""));
        assert!(update.contains("ON CONFLICT (\"Id\") DO UPDATE SET"));
        assert!(update.contains("\"Name\"=EXCLUDED.\"Name\""));
        assert!(update.contains("\"OwnerId\"=EXCLUDED.\"OwnerId\""));
        assert!(!update.contains("\"Id\"=EXCLUDED")); // never reassigns the key
        assert!(!update.contains("bad; drop")); // injection guard
        // Insert-only leaves an existing row untouched.
        assert!(upsert_sql("Campaign", &row, false).contains("ON CONFLICT (\"Id\") DO NOTHING"));
    }
    #[test] fn local_image_scan_finds_known_patterns() { let rows=vec![serde_json::json!({"Id":"1","Body":"/0/rest/ImageService/GetFile"})]; assert_eq!(local_image_hits("BulkEmail",&rows),["BulkEmail: 1"]); }
    #[test]
    fn plan_row_default_migrates_only_missing() {
        // No explicit selection: missing rows are inserted, existing ones skipped.
        assert_eq!(plan_row(false, None, false), RowPlan::Process { update: false });
        assert_eq!(plan_row(true, None, false), RowPlan::Skip);
        // Overwrite without a selection still never touches existing rows.
        assert_eq!(plan_row(true, None, true), RowPlan::Skip);
    }

    #[test]
    fn plan_row_selection_gates_inserts() {
        assert_eq!(plan_row(false, Some(true), false), RowPlan::Process { update: false });
        assert_eq!(plan_row(false, Some(false), false), RowPlan::NotSelected);
    }

    #[test]
    fn plan_row_overwrite_updates_selected_existing() {
        // Selected + existing + overwrite → update; without overwrite → skip.
        assert_eq!(plan_row(true, Some(true), true), RowPlan::Process { update: true });
        assert_eq!(plan_row(true, Some(true), false), RowPlan::Skip);
        // An existing row left unselected is skipped regardless of overwrite.
        assert_eq!(plan_row(true, Some(false), true), RowPlan::Skip);
    }

    #[test] fn campaign_item_sql_preserves_empty_name_and_escapes() { let rows=vec![serde_json::json!({"Id":"i","Name":"","CampaignElementType":"it's","IsDeleted":false})]; let sql=campaign_item_sql(&rows,"sup"); assert!(sql.contains("'it''s'")); assert!(sql.contains("VALUES ('i',NULL,''")); assert!(sql.contains("ON CONFLICT")); }

    const G_OWNER: &str = "11111111-2222-3333-4444-555555555555";
    const G_KNOWN: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    const G_NEW: &str = "99999999-8888-7777-6666-555555555555";

    #[test]
    fn guid_shapes_are_detected() {
        assert!(is_guid(G_OWNER));
        assert!(is_guid(ZERO_GUID));
        assert!(!is_guid("not-a-guid"));
        assert!(!is_guid("11111111-2222-3333-4444-55555555555")); // 35 chars
        assert!(!is_guid("11111111x2222-3333-4444-555555555555"));
    }

    #[test]
    fn resolve_row_remaps_clears_and_keeps() {
        let rules = vec![
            FkRule { column: "OwnerId".into(), ref_table: "Contact".into() },
            FkRule { column: "TypeId".into(), ref_table: "CampaignType".into() },
            FkRule { column: "StatusId".into(), ref_table: "CampaignStatus".into() },
        ];
        let mut body = serde_json::json!({"Id":"x","OwnerId":G_OWNER,"TypeId":G_KNOWN,"StatusId":G_KNOWN,"Name":"c"}).as_object().unwrap().clone();
        let (adjustments, blocked) = resolve_row("x", "c", &mut body, &rules, &mut |rule, _value| match rule.column.as_str() {
            "OwnerId" => FkDecision::Remap(G_NEW.into(), "by name".into()),
            "TypeId" => FkDecision::Clear("missing".into()),
            _ => FkDecision::Keep,
        });
        assert!(blocked.is_none());
        assert_eq!(body.get("OwnerId").and_then(Value::as_str), Some(G_NEW));
        assert!(body.get("TypeId").is_none());
        assert_eq!(body.get("StatusId").and_then(Value::as_str), Some(G_KNOWN));
        assert_eq!(adjustments.len(), 2);
        assert_eq!(adjustments[0].action, "remapped");
        assert_eq!(adjustments[1].action, "cleared");
    }

    #[test]
    fn resolve_row_blocks_on_missing_content_parent() {
        let rules = vec![FkRule { column: "CampaignId".into(), ref_table: "Campaign".into() }];
        let mut body = serde_json::json!({"Id":"x","CampaignId":G_OWNER}).as_object().unwrap().clone();
        let (_, blocked) = resolve_row("x", "b", &mut body, &rules, &mut |_, _| FkDecision::Block("parent missing".into()));
        assert_eq!(blocked.as_deref(), Some("parent missing"));
    }

    #[test]
    fn resolve_row_ignores_non_guid_and_absent_fields() {
        let rules = vec![FkRule { column: "OwnerId".into(), ref_table: "Contact".into() }, FkRule { column: "Gone".into(), ref_table: "X".into() }];
        let mut body = serde_json::json!({"Id":"x","OwnerId":"free text"}).as_object().unwrap().clone();
        let (adjustments, blocked) = resolve_row("x", "n", &mut body, &rules, &mut |_, _| FkDecision::Clear("should not run".into()));
        assert!(adjustments.is_empty() && blocked.is_none());
        assert_eq!(body.get("OwnerId").and_then(Value::as_str), Some("free text"));
    }

    #[test]
    fn parent_campaigns_insert_before_children() {
        let parent = serde_json::json!({"Id":"aaaaaaaa-0000-0000-0000-000000000001","Name":"parent"});
        let child = serde_json::json!({"Id":"aaaaaaaa-0000-0000-0000-000000000002","TwkParentCampaignId":"AAAAAAAA-0000-0000-0000-000000000001","Name":"child"});
        let ordered = order_rows_parents_first(vec![child.clone(), parent.clone()], &["TwkParentCampaignId".to_string()]);
        assert_eq!(name(&ordered[0]), "parent");
        assert_eq!(name(&ordered[1]), "child");
        // A cycle falls back to the original order instead of hanging.
        let a = serde_json::json!({"Id":"aaaaaaaa-0000-0000-0000-000000000003","TwkParentCampaignId":"aaaaaaaa-0000-0000-0000-000000000004"});
        let b = serde_json::json!({"Id":"aaaaaaaa-0000-0000-0000-000000000004","TwkParentCampaignId":"aaaaaaaa-0000-0000-0000-000000000003"});
        assert_eq!(order_rows_parents_first(vec![a.clone(), b.clone()], &["TwkParentCampaignId".to_string()]).len(), 2);
    }

    #[test]
    fn selection_closure_pulls_in_required_parents() {
        let campaign_id = "aaaaaaaa-0000-0000-0000-00000000000a";
        let email_id = "aaaaaaaa-0000-0000-0000-00000000000b";
        let order = vec!["Campaign".to_string(), "BulkEmail".to_string()];
        let rules = BTreeMap::from([(
            "BulkEmail".to_string(),
            vec![FkRule { column: "CampaignId".into(), ref_table: "Campaign".into() }],
        )]);
        let source_rows = BTreeMap::from([
            ("Campaign".to_string(), vec![serde_json::json!({"Id":campaign_id,"Name":"c"})]),
            ("BulkEmail".to_string(), vec![serde_json::json!({"Id":email_id,"CampaignId":campaign_id,"Name":"e"})]),
        ]);
        let target_ids = BTreeMap::from([
            ("Campaign".to_string(), BTreeSet::new()),
            ("BulkEmail".to_string(), BTreeSet::new()),
        ]);
        // The user picked one bulk email and no campaigns.
        let mut chosen = BTreeMap::from([
            ("Campaign".to_string(), Some(BTreeSet::new())),
            ("BulkEmail".to_string(), Some(BTreeSet::from([email_id.to_string()]))),
        ]);
        let added = close_over_parents(&order, &rules, &source_rows, &target_ids, &mut chosen);
        assert!(chosen["Campaign"].as_ref().unwrap().contains(campaign_id));
        assert!(added["Campaign"].contains(campaign_id));
        // A parent already on the target is not re-added.
        let target_ids = BTreeMap::from([
            ("Campaign".to_string(), BTreeSet::from([campaign_id.to_string()])),
            ("BulkEmail".to_string(), BTreeSet::new()),
        ]);
        let mut chosen = BTreeMap::from([
            ("Campaign".to_string(), Some(BTreeSet::new())),
            ("BulkEmail".to_string(), Some(BTreeSet::from([email_id.to_string()]))),
        ]);
        let added = close_over_parents(&order, &rules, &source_rows, &target_ids, &mut chosen);
        assert!(added.is_empty());
        assert!(chosen["Campaign"].as_ref().unwrap().is_empty());
    }
}
