//! GUID-preserving marketing-content migration over Creatio OData.
//!
//! Ordinary business rows can be inserted through OData. Campaign flow
//! `SysSchema` rows are protected, so their lossless media payloads are read
//! from OData and inserted on the target through the existing cliogate SQL path.

use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMigrateResult {
    pub entity: String,
    pub source_count: usize,
    pub inserted: usize,
    pub skipped: usize,
    pub failures: Vec<RowFailure>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMigrateReport {
    pub source_env: String,
    pub target_env: String,
    pub rollback_path: String,
    pub entities: Vec<EntityMigrateResult>,
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

pub fn flow_insert_sql(schema: &Value, metadata: &[u8], descriptor: &[u8], supervisor: &str) -> String {
    let get = |key: &str| schema.get(key).and_then(Value::as_str).unwrap_or_default();
    let caption: String = get("Caption").chars().take(250).collect();
    format!("INSERT INTO \"SysSchema\" (\"Id\",\"UId\",\"Name\",\"Caption\",\"ManagerName\",\"SysPackageId\",\"IsLocked\",\"IsChanged\",\"ExtendParent\",\"Description\",\"Checksum\",\"IsNetStandard\",\"CreatedById\",\"ModifiedById\",\"MetaData\",\"Descriptor\",\"MetaDataModifiedOn\",\"StructureModifiedOn\") VALUES ('{}','{}','{}','{}','CampaignSchemaManager','{}',false,true,false,'{}','{}',{},'{}','{}',decode('{}','hex'),decode('{}','hex'),timezone('utc'::text,CURRENT_TIMESTAMP),timezone('utc'::text,CURRENT_TIMESTAMP)) ON CONFLICT (\"Id\") DO NOTHING;",
        sql_escape(get("Id")), sql_escape(get("UId")), sql_escape(get("Name")), sql_escape(&caption), sql_escape(get("SysPackageId")), sql_escape(get("Description")), sql_escape(get("Checksum")),
        schema.get("IsNetStandard").and_then(Value::as_bool).unwrap_or(true), supervisor, supervisor, hex(metadata), hex(descriptor))
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

fn copy_entity(source: &Connection, target: &Connection, entity: &str, source_supervisor: &str, target_supervisor: &str, extra_drop: &[&str], sql_empty_names: bool) -> Result<(EntityMigrateResult, Vec<Value>), String> {
    let source_rows = source.get_all(entity, None, None)?;
    let target_rows = target.get_all(entity, Some("Id"), None)?;
    let missing = missing_ids(&source_rows, &target_rows);
    let todo: Vec<Value> = source_rows.iter().filter(|row| missing.contains(&id(row).to_ascii_lowercase())).cloned().collect();
    let mut inserted = 0; let mut failures = Vec::new(); let mut sql_rows = Vec::new();
    for row in &todo {
        if sql_empty_names && row.get("Name").and_then(Value::as_str).unwrap_or_default().is_empty() { sql_rows.push(row.clone()); continue; }
        let body = clean_row(row, Some(source_supervisor), Some(target_supervisor), extra_drop);
        match target.post(entity, &body) {
            Ok(()) => inserted += 1,
            Err(error) => failures.push(RowFailure { id: id(row), name: name(row), status: error.status, error: error.message }),
        }
    }
    Ok((EntityMigrateResult { entity: entity.to_string(), source_count: source_rows.len(), inserted, skipped: source_rows.len() - todo.len(), failures }, sql_rows))
}

#[tauri::command]
pub fn content_analyze(source_env: String, target_env: String) -> Result<ContentGapReport, String> { analyze(&source_env, &target_env) }

#[tauri::command]
pub fn content_verify(source_env: String, target_env: String) -> Result<ContentGapReport, String> { analyze(&source_env, &target_env) }

#[tauri::command]
pub fn content_migrate(app: AppHandle, source_env: String, target_env: String, entities: Vec<String>) -> Result<ContentMigrateReport, String> {
    if source_env == target_env { return Err("Choose two different environments.".to_string()); }
    let order = dependency_order(&entities)?;
    if order.is_empty() { return Err("Select at least one content entity.".to_string()); }
    let source = Connection::connect(&source_env)?; let target = Connection::connect(&target_env)?;
    let source_supervisor = supervisor_id(&source)?; let target_supervisor = supervisor_id(&target)?;
    let mut prepared = Vec::new(); let mut rollback_batches = Vec::new();
    for entity in &order {
        let rows = source.get_all(entity, None, None)?; let target_rows = target.get_all(entity, Some("Id"), None)?; let missing = missing_ids(&rows, &target_rows);
        rollback_batches.push((entity.clone(), rows.iter().filter(|r| missing.contains(&id(r).to_ascii_lowercase())).map(id).collect()));
        prepared.push((entity.clone(), rows));
    }
    let rollback = rollback_path(&app, &target_env, "rows")?; write_delete_rollback(&rollback, &rollback_batches)?;
    let mut results = Vec::new();
    for (entity, _) in prepared {
        let drop = if entity == "Campaign" { &["CampaignSchemaUId"][..] } else { &[][..] };
        results.push(copy_entity(&source, &target, &entity, &source_supervisor, &target_supervisor, drop, false)?.0);
    }
    Ok(ContentMigrateReport { source_env, target_env, rollback_path: rollback.to_string_lossy().to_string(), entities: results })
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
    let mut sql = String::from("BEGIN;\n"); let mut metadata_bytes = 0;
    for schema in &missing_schemas { let metadata = source.media(&id(schema), "MetaData")?; let descriptor = source.media(&id(schema), "Descriptor")?; metadata_bytes += metadata.len(); sql.push_str(&flow_insert_sql(schema, &metadata, &descriptor, &target_supervisor)); sql.push('\n'); }
    sql.push_str("COMMIT;\n"); if !missing_schemas.is_empty() { run_sql(&target_env, &sql)?; }
    for schema in &missing_schemas {
        let source_len = source.media(&id(schema), "MetaData")?.len();
        let target_len = target.media(&id(schema), "MetaData")?.len();
        if source_len != target_len { return Err(format!("Flow {} MetaData length differs after insert ({source_len} vs {target_len} bytes).", id(schema))); }
    }
    let mut failures = Vec::new(); let mut repointed = 0;
    for campaign in &campaigns { let campaign_id = id(campaign); let uid = campaign.get("CampaignSchemaUId").and_then(Value::as_str).unwrap_or_default(); match target.patch("Campaign", &campaign_id, &serde_json::json!({"CampaignSchemaUId": uid})) { Ok(()) => repointed += 1, Err(e) => failures.push(RowFailure { id: campaign_id, name: name(campaign), status: e.status, error: e.message }) } }
    let mut results = Vec::new();
    let (versions, _) = copy_entity(&source, &target, "CampaignVersion", &source_supervisor, &target_supervisor, &[], false)?; results.push(versions);
    let (mut items, sql_items) = copy_entity(&source, &target, "CampaignItem", &source_supervisor, &target_supervisor, &[], true)?;
    if !sql_items.is_empty() { run_sql(&target_env, &format!("BEGIN;\n{}\nCOMMIT;", campaign_item_sql(&sql_items, &target_supervisor)))?; items.inserted += sql_items.len(); }
    results.push(items);
    // Captions are scoped strictly to the migrated flow schema rows.
    let slv_source = flow_localizable_values(&source, &schemas, None)?;
    let slv_target = flow_localizable_values(&target, &schemas, Some("Id"))?;
    let slv_missing = missing_ids(&slv_source, &slv_target); let mut slv_result = EntityMigrateResult { entity: "SysLocalizableValue".to_string(), source_count: slv_source.len(), inserted: 0, skipped: slv_source.len() - slv_missing.len(), failures: Vec::new() };
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
    #[test] fn flow_sql_escapes_quotes_unicode_and_hex() { let s=serde_json::json!({"Id":"i","UId":"u","Name":"O'Brien — flow","Caption":"It's — live","SysPackageId":"p","Description":"d's","Checksum":"c","IsNetStandard":true}); let sql=flow_insert_sql(&s,b"{}",&[0xff],"sup"); assert!(sql.contains("O''Brien — flow")); assert!(sql.contains("It''s — live")); assert!(sql.contains("decode('7b7d','hex')")); assert!(sql.contains("decode('ff','hex')")); }
    #[test] fn local_image_scan_finds_known_patterns() { let rows=vec![serde_json::json!({"Id":"1","Body":"/0/rest/ImageService/GetFile"})]; assert_eq!(local_image_hits("BulkEmail",&rows),["BulkEmail: 1"]); }
    #[test] fn campaign_item_sql_preserves_empty_name_and_escapes() { let rows=vec![serde_json::json!({"Id":"i","Name":"","CampaignElementType":"it's","IsDeleted":false})]; let sql=campaign_item_sql(&rows,"sup"); assert!(sql.contains("'it''s'")); assert!(sql.contains("VALUES ('i',NULL,''")); assert!(sql.contains("ON CONFLICT")); }
}
