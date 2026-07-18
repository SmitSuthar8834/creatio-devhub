use crate::clio;
use crate::git;
use crate::jobs::{now_ms, JobState};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub env: String,
    #[serde(default)]
    pub app_code: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub last_pull: Option<u64>,
    #[serde(default)]
    pub last_push: Option<u64>,
    /// Package name|version rows recorded at last pull/push — the drift baseline.
    #[serde(default)]
    pub packages_snapshot: Option<String>,
}

/// Live git state computed on demand, merged into the workspace card.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub exists: bool,
    pub branch: Option<String>,
    pub remote: Option<String>,
    pub dirty_count: usize,
}

/// Registry backed by workspaces.json in the app data dir.
/// The file is the single source of truth; the mutex serializes read-modify-write.
#[derive(Clone)]
pub struct WsState {
    file: PathBuf,
    io: Arc<Mutex<()>>,
}

impl WsState {
    pub fn load(app: &AppHandle) -> Self {
        let dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
        let _ = std::fs::create_dir_all(&dir);
        WsState { file: dir.join("workspaces.json"), io: Arc::new(Mutex::new(())) }
    }

    fn read(&self) -> Vec<Workspace> {
        let _g = self.io.lock().unwrap();
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn modify(&self, f: impl FnOnce(&mut Vec<Workspace>)) {
        let _g = self.io.lock().unwrap();
        let mut list: Vec<Workspace> = std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        f(&mut list);
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(&self.file, json);
        }
    }

    fn get(&self, id: &str) -> Result<Workspace, String> {
        self.read()
            .into_iter()
            .find(|w| w.id == id)
            .ok_or_else(|| "Workspace not found".to_string())
    }
}

const GITIGNORE: &str = "\
# Creatio DevHub workspace — build artifacts and downloaded binaries stay local
.application/
.nuget/
bin/
obj/
*.user
";

fn summarize(w: &Workspace) -> WorkspaceSummary {
    let path = Path::new(&w.path);
    let exists = path.is_dir();
    let (branch, remote, dirty_count) = if exists {
        (
            git::current_branch(path),
            git::remote_url(path),
            git::status(path).map(|c| c.len()).unwrap_or(0),
        )
    } else {
        (None, None, 0)
    };
    WorkspaceSummary { workspace: w.clone(), exists, branch, remote, dirty_count }
}

#[tauri::command]
pub fn list_workspaces(ws: State<'_, WsState>) -> Vec<WorkspaceSummary> {
    ws.read().iter().map(summarize).collect()
}

/// Register an already-existing workspace folder (e.g. a fresh git clone).
#[tauri::command]
pub fn register_workspace(
    app: AppHandle,
    ws: State<'_, WsState>,
    path: String,
    env: String,
) -> Result<WorkspaceSummary, String> {
    let dir = PathBuf::from(&path);
    if !dir.join(".clio").is_dir() {
        return Err("That folder is not a clio workspace (missing .clio directory).".to_string());
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());
    let w = Workspace {
        id: format!("ws-{}", now_ms()),
        name,
        path,
        env,
        app_code: None,
        created_at: now_ms(),
        last_pull: None,
        last_push: None,
        packages_snapshot: None,
    };
    ws.modify(|list| list.push(w.clone()));
    let _ = app.emit("workspaces-changed", ());
    Ok(summarize(&w))
}

#[tauri::command]
pub fn remove_workspace(app: AppHandle, ws: State<'_, WsState>, id: String) -> Result<(), String> {
    ws.modify(|list| list.retain(|w| w.id != id));
    let _ = app.emit("workspaces-changed", ());
    Ok(()) // folder on disk is left untouched
}

/// Full "new workspace" flow as one streamed job:
/// scaffold → restore from environment → git init → initial commit → optional remote push.
#[tauri::command]
pub fn create_workspace_flow(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    name: String,
    parent_dir: String,
    env: String,
    app_code: Option<String>,
    remote_url: Option<String>,
) -> Result<String, String> {
    let dir = PathBuf::from(&parent_dir).join(&name);
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("Folder {} already exists and is not empty.", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;

    let id = jobs.create_job(&app, "create-workspace", Some(env.clone()), format!("new workspace {name} ← {env}"));
    let lock = jobs.env_lock(Some(&env));
    let st = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let job_id = id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !st.mark_running_phase(&app, &job_id, "creating workspace", true) {
            return;
        }

        let result = (|| -> Result<(), String> {
            let mut args: Vec<String> = vec!["create-workspace".into(), "-e".into(), env.clone()];
            if let Some(code) = &app_code {
                args.push("-a".into());
                args.push(code.clone());
            }
            if st.stream_process(&app, &job_id, "clio", &args, Some(&dir), &[])? != 0 {
                return Err("clio create-workspace failed".to_string());
            }

            if st.is_cancel_requested(&job_id) {
                return Err("Cancelled before restore.".to_string());
            }
            if !st.set_phase(&app, &job_id, "restoring workspace files", true) {
                return Err("Cancelled before restore.".to_string());
            }
            let restore: Vec<String> = vec!["restore-workspace".into(), "-e".into(), env.clone()];
            if st.stream_process(&app, &job_id, "clio", &restore, Some(&dir), &[])? != 0 {
                return Err("clio restore-workspace failed".to_string());
            }

            st.log(&app, &job_id, "Initializing git repository…");
            git::git_ok(&dir, &["init", "-b", "main"])?;
            std::fs::write(dir.join(".gitignore"), GITIGNORE).map_err(|e| e.to_string())?;
            git::git_ok(&dir, &["add", "-A"])?;
            git::git_ok(&dir, &["commit", "-m", &format!("Initial pull from {env}")])?;
            st.log(&app, &job_id, "✓ Initial commit created on branch main");

            if let Some(url) = remote_url.as_ref().filter(|u| !u.trim().is_empty()) {
                git::git_ok(&dir, &["remote", "add", "origin", url.trim()])?;
                st.log(&app, &job_id, format!("Pushing to {}…", url.trim()));
                match git::git_ok(&dir, &["push", "-u", "origin", "main"]) {
                    Ok(_) => st.log(&app, &job_id, "✓ Pushed to remote"),
                    Err(e) => st.log(
                        &app,
                        &job_id,
                        format!("⚠ Remote push failed ({e}). The workspace is fine — set up git credentials and push from the workspace screen."),
                    ),
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                let snapshot = clio::packages_snapshot(&env).ok();
                let w = Workspace {
                    id: format!("ws-{}", now_ms()),
                    name: name.clone(),
                    path: dir.to_string_lossy().to_string(),
                    env: env.clone(),
                    app_code,
                    created_at: now_ms(),
                    last_pull: Some(now_ms()),
                    last_push: None,
                    packages_snapshot: snapshot,
                };
                wstate.modify(|list| list.push(w));
                let _ = app.emit("workspaces-changed", ());
                st.log(&app, &job_id, "✓ Workspace ready");
                st.finish(&app, &job_id, Some(0));
            }
            Err(e) => {
                st.log(&app, &job_id, format!("✗ {e}"));
                st.finish(&app, &job_id, Some(1));
            }
        }
    });

    Ok(id)
}

/// Pull from Cloud: require a clean tree, then restore-workspace and report what changed.
#[tauri::command]
pub fn pull_workspace(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    id: String,
) -> Result<String, String> {
    let w = ws.get(&id)?;
    let dir = PathBuf::from(&w.path);
    if !dir.is_dir() {
        return Err(format!("Workspace folder is missing: {}", w.path));
    }
    let dirty = git::status(&dir)?.len();
    if dirty > 0 {
        return Err(format!(
            "The workspace has {dirty} uncommitted change(s). Commit or discard them before pulling, so cloud changes never mix with local edits."
        ));
    }

    let job_id = jobs.create_job(&app, "pull-from-cloud", Some(w.env.clone()), format!("pull {} ← {}", w.name, w.env));
    let lock = jobs.env_lock(Some(&w.env));
    let st = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let jid = job_id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !st.mark_running_phase(&app, &jid, "restoring workspace files", true) {
            return;
        }
        let restore: Vec<String> = vec!["restore-workspace".into(), "-e".into(), w.env.clone()];
        match st.stream_process(&app, &jid, "clio", &restore, Some(&dir), &[]) {
            Ok(0) => {
                let changed = git::status(&dir).map(|c| c.len()).unwrap_or(0);
                if changed == 0 {
                    st.log(&app, &jid, "✓ Already up to date — no changes in the cloud since last pull.");
                } else {
                    st.log(&app, &jid, format!("✓ Pulled — {changed} file(s) changed. Review and commit them on the workspace screen."));
                }
                let snapshot = clio::packages_snapshot(&w.env).ok();
                wstate.modify(|list| {
                    if let Some(entry) = list.iter_mut().find(|x| x.id == w.id) {
                        entry.last_pull = Some(now_ms());
                        if snapshot.is_some() {
                            entry.packages_snapshot = snapshot.clone();
                        }
                    }
                });
                let _ = app.emit("workspaces-changed", ());
                st.finish(&app, &jid, Some(0));
            }
            Ok(code) => st.finish(&app, &jid, Some(code)),
            Err(e) => {
                st.log(&app, &jid, e);
                st.finish(&app, &jid, Some(1));
            }
        }
    });

    Ok(job_id)
}

fn workspace_has_package(dir: &Path, package: &str) -> bool {
    let settings = dir.join(".clio").join("workspaceSettings.json");
    let Ok(raw) = std::fs::read_to_string(settings) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    json.get("Packages")
        .or_else(|| json.get("packages"))
        .and_then(|value| value.as_array())
        .is_some_and(|packages| {
            packages.iter().any(|item| {
                item.as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(package))
            })
        })
}

/// Append an existing cloud package to a workspace's managed package selection,
/// then restore the workspace so the package becomes a normal Git change.
#[tauri::command]
pub fn add_package_to_workspace(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    id: String,
    package: String,
) -> Result<String, String> {
    let w = ws.get(&id)?;
    let package = package.trim().to_string();
    if package.is_empty() || package.contains(',') || package.starts_with('-') {
        return Err("Invalid package name.".to_string());
    }
    let dir = PathBuf::from(&w.path);
    if !dir.is_dir() {
        return Err(format!("Workspace folder is missing: {}", w.path));
    }
    if !dir.join(".clio").join("workspaceSettings.json").is_file() {
        return Err("The selected folder is not a configured clio workspace.".to_string());
    }
    if workspace_has_package(&dir, &package) {
        return Err(format!("{package} is already included in {}.", w.name));
    }
    let dirty = git::status(&dir)?.len();
    if dirty > 0 {
        return Err(format!(
            "The workspace has {dirty} uncommitted change(s). Commit or discard them before adding another package, so unrelated changes never mix."
        ));
    }

    let job_id = jobs.create_job(
        &app,
        "add-package-to-workspace",
        Some(w.env.clone()),
        format!("add {package} to workspace {}", w.name),
    );
    let lock = jobs.env_lock(Some(&w.env));
    let state = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let jid = job_id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !state.mark_running_phase(&app, &jid, "updating workspace selection", true) {
            return;
        }
        let result = (|| -> Result<i32, String> {
            state.log(
                &app,
                &jid,
                format!("Adding {package} to the workspace package selection…"),
            );
            let configure = vec![
                "cfg-worspace".into(),
                "--Packages".into(),
                package.clone(),
                "-e".into(),
                w.env.clone(),
            ];
            let code =
                state.stream_process(&app, &jid, "clio", &configure, Some(&dir), &[])?;
            if code != 0 {
                return Ok(code);
            }

            if state.is_cancel_requested(&jid) {
                return Ok(1);
            }
            if !state.set_phase(&app, &jid, "restoring package into workspace", true) {
                return Ok(1);
            }
            state.log(
                &app,
                &jid,
                format!("Restoring {package} source files from {}…", w.env),
            );
            let restore = vec!["restore-workspace".into(), "-e".into(), w.env.clone()];
            let code = state.stream_process(&app, &jid, "clio", &restore, Some(&dir), &[])?;
            if code == 0 {
                let changed = git::status(&dir).map(|items| items.len()).unwrap_or(0);
                state.log(
                    &app,
                    &jid,
                    format!(
                        "✓ {package} is now part of {}. Review the {changed} changed file(s), then commit and push them to Git.",
                        w.name
                    ),
                );
                let snapshot = clio::packages_snapshot(&w.env).ok();
                wstate.modify(|list| {
                    if let Some(entry) = list.iter_mut().find(|entry| entry.id == w.id) {
                        entry.last_pull = Some(now_ms());
                        if snapshot.is_some() {
                            entry.packages_snapshot = snapshot.clone();
                        }
                    }
                });
                let _ = app.emit("workspaces-changed", ());
            }
            Ok(code)
        })();
        match result {
            Ok(code) => state.finish(&app, &jid, Some(code)),
            Err(error) => {
                state.log(&app, &jid, error);
                state.finish(&app, &jid, Some(1));
            }
        }
    });

    Ok(job_id)
}

#[tauri::command]
pub fn ws_status(ws: State<'_, WsState>, id: String) -> Result<Vec<git::FileChange>, String> {
    let w = ws.get(&id)?;
    git::status(Path::new(&w.path))
}

#[tauri::command]
pub fn ws_diff(ws: State<'_, WsState>, id: String, file: String) -> Result<String, String> {
    let w = ws.get(&id)?;
    let dir = Path::new(&w.path);
    // HEAD-relative diff covers modified files; untracked files get shown whole.
    let (code, out, _) = git::git(dir, &["diff", "HEAD", "--", &file])?;
    if code == 0 && !out.trim().is_empty() {
        return Ok(out);
    }
    let content = std::fs::read_to_string(dir.join(&file)).unwrap_or_default();
    Ok(content
        .lines()
        .map(|l| format!("+{l}"))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[tauri::command]
pub fn ws_log(ws: State<'_, WsState>, id: String) -> Result<Vec<git::Commit>, String> {
    let w = ws.get(&id)?;
    git::log(Path::new(&w.path), 50)
}

#[tauri::command]
pub fn ws_commit(app: AppHandle, ws: State<'_, WsState>, id: String, message: String) -> Result<String, String> {
    let w = ws.get(&id)?;
    let dir = Path::new(&w.path);
    if message.trim().is_empty() {
        return Err("Commit message is required.".to_string());
    }
    git::git_ok(dir, &["add", "-A"])?;
    let out = git::git_ok(dir, &["commit", "-m", message.trim()])?;
    let _ = app.emit("workspaces-changed", ());
    Ok(out.lines().next().unwrap_or("committed").to_string())
}

#[tauri::command]
pub fn ws_set_remote(ws: State<'_, WsState>, id: String, url: String) -> Result<(), String> {
    let w = ws.get(&id)?;
    let dir = Path::new(&w.path);
    if git::remote_url(dir).is_some() {
        git::git_ok(dir, &["remote", "set-url", "origin", url.trim()])?;
    } else {
        git::git_ok(dir, &["remote", "add", "origin", url.trim()])?;
    }
    Ok(())
}

#[tauri::command]
pub fn ws_remote_status(
    ws: State<'_, WsState>,
    id: String,
) -> Result<git::RemoteStatus, String> {
    let w = ws.get(&id)?;
    git::remote_status(Path::new(&w.path))
}

/// Deploy the workspace to its Creatio environment (clio push-workspace).
/// Unless `force`, first compares the environment's current package list with the
/// snapshot recorded at last pull — if it changed, someone else modified the cloud
/// and we refuse with a DRIFT error so the UI can offer "pull first / push anyway".
#[tauri::command]
pub fn push_workspace_cloud(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    id: String,
    force: bool,
    skip_backup: bool,
) -> Result<String, String> {
    let w = ws.get(&id)?;
    let dir = PathBuf::from(&w.path);
    if !dir.is_dir() {
        return Err(format!("Workspace folder is missing: {}", w.path));
    }

    if !force {
        if let Some(baseline) = &w.packages_snapshot {
            match clio::packages_snapshot(&w.env) {
                Ok(current) if &current != baseline => {
                    let base: std::collections::HashSet<&str> = baseline.lines().collect();
                    let cur: std::collections::HashSet<&str> = current.lines().collect();
                    let changed: Vec<String> = cur
                        .symmetric_difference(&base)
                        .map(|r| r.replace('|', " "))
                        .take(6)
                        .collect();
                    return Err(format!(
                        "DRIFT: {} has package changes you haven't pulled ({}). Pull first so those changes reach git, or push anyway to proceed regardless.",
                        w.env,
                        changed.join(", ")
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    // Can't read the env package list — surface it, but don't block the push.
                    eprintln!("drift check skipped: {e}");
                }
            }
        }
    }

    let job_id = jobs.create_job(&app, "push-to-cloud", Some(w.env.clone()), format!("push {} → {}", w.name, w.env));
    let lock = jobs.env_lock(Some(&w.env));
    let st = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let jid = job_id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !st.mark_running_phase(
            &app,
            &jid,
            "installing workspace in environment",
            false,
        ) {
            return;
        }
        st.log(&app, &jid, "Packing workspace and installing to the environment — the server-side compile can take several minutes. This job cannot be safely cancelled once installation starts.");
        let mut args: Vec<String> = vec!["push-workspace".into(), "-e".into(), w.env.clone()];
        if skip_backup {
            args.push("--skip-backup".into());
            args.push("true".into());
        }
        match st.stream_process(&app, &jid, "clio", &args, Some(&dir), &[]) {
            Ok(0) => {
                st.log(&app, &jid, "✓ Workspace installed to the environment.");
                let snapshot = clio::packages_snapshot(&w.env).ok();
                wstate.modify(|list| {
                    if let Some(entry) = list.iter_mut().find(|x| x.id == w.id) {
                        entry.last_push = Some(now_ms());
                        if snapshot.is_some() {
                            entry.packages_snapshot = snapshot.clone();
                        }
                    }
                });
                let _ = app.emit("workspaces-changed", ());
                st.finish(&app, &jid, Some(0));
            }
            Ok(code) => {
                st.log(&app, &jid, "✗ Push failed — scroll up for the compile/installation errors. The environment keeps its backup unless you skipped it.");
                st.finish(&app, &jid, Some(code));
            }
            Err(e) => {
                st.log(&app, &jid, e);
                st.finish(&app, &jid, Some(1));
            }
        }
    });

    Ok(job_id)
}

/// Push commits to the git remote as a streamed job (may take a moment on first push).
#[tauri::command]
pub fn ws_push_remote(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    id: String,
) -> Result<String, String> {
    let w = ws.get(&id)?;
    let dir = PathBuf::from(&w.path);
    if git::remote_url(&dir).is_none() {
        return Err("No git remote configured. Set a remote URL first.".to_string());
    }
    let remote = git::remote_status(&dir)?;
    if remote.behind > 0 {
        return Err(format!(
            "REMOTE_AHEAD: origin has {} commit(s) you do not have. Another contributor pushed changes; pull/rebase before pushing to avoid conflicts.",
            remote.behind
        ));
    }
    let branch = git::current_branch(&dir).unwrap_or_else(|| "main".to_string());
    let job_id = jobs.create_job(&app, "git-push", Some(w.env.clone()), format!("git push origin {branch}"));
    let st = jobs.inner().clone();
    let jid = job_id.clone();

    std::thread::spawn(move || {
        if !st.mark_running_phase(&app, &jid, "pushing commits to remote", false) {
            return;
        }
        let args: Vec<String> = vec!["push".into(), "-u".into(), "origin".into(), branch];
        match st.stream_process(&app, &jid, "git", &args, Some(&dir), &[]) {
            Ok(code) => {
                if code != 0 {
                    st.log(&app, &jid, "Push failed. If this is an authentication error, sign in once with your git client (e.g. `git push` in a terminal) so Windows credential manager stores the token.");
                }
                st.finish(&app, &jid, Some(code));
            }
            Err(e) => {
                st.log(&app, &jid, e);
                st.finish(&app, &jid, Some(1));
            }
        }
    });

    Ok(job_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_configured_workspace_packages_case_insensitively() {
        let dir = std::env::temp_dir().join(format!(
            "devhub-workspace-settings-test-{}",
            std::process::id()
        ));
        let clio_dir = dir.join(".clio");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&clio_dir).unwrap();
        std::fs::write(
            clio_dir.join("workspaceSettings.json"),
            r#"{"Packages":["Base","QntImportEngine"],"IgnorePackages":[]}"#,
        )
        .unwrap();

        assert!(workspace_has_package(&dir, "qntimportengine"));
        assert!(!workspace_has_package(&dir, "AnotherPackage"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
