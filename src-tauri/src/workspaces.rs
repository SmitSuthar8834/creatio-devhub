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
    skip_restore: bool,
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

            if skip_restore {
                st.log(&app, &job_id, "Empty workspace — skipping package download. Add packages later from the workspace screen.");
            } else {
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
            }

            st.log(&app, &job_id, "Initializing git repository…");
            git::git_ok(&dir, &["init", "-b", "main"])?;
            std::fs::write(dir.join(".gitignore"), GITIGNORE).map_err(|e| e.to_string())?;
            git::git_ok(&dir, &["add", "-A"])?;
            let commit_msg = if skip_restore {
                "Initial empty workspace".to_string()
            } else {
                format!("Initial pull from {env}")
            };
            git::git_ok(&dir, &["commit", "-m", &commit_msg])?;
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
                // An empty workspace has pulled nothing, so it has no drift baseline yet;
                // the first add-package/pull records the snapshot.
                let snapshot = if skip_restore { None } else { clio::packages_snapshot(&env).ok() };
                let w = Workspace {
                    id: format!("ws-{}", now_ms()),
                    name: name.clone(),
                    path: dir.to_string_lossy().to_string(),
                    env: env.clone(),
                    app_code,
                    created_at: now_ms(),
                    last_pull: if skip_restore { None } else { Some(now_ms()) },
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

/// Package names selected in the workspace, read from
/// `.clio/workspaceSettings.json`. Empty when the file is missing or unreadable
/// (callers treat that as "no packages" rather than an error).
fn read_workspace_packages(dir: &Path) -> Vec<String> {
    let settings = dir.join(".clio").join("workspaceSettings.json");
    let Ok(raw) = std::fs::read_to_string(settings) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    json.get("Packages")
        .or_else(|| json.get("packages"))
        .and_then(|value| value.as_array())
        .map(|packages| {
            packages
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn workspace_has_package(dir: &Path, package: &str) -> bool {
    read_workspace_packages(dir)
        .iter()
        .any(|name| name.eq_ignore_ascii_case(package))
}

/// The packages a workspace version-controls, sorted case-insensitively for the
/// Packages tab. Missing settings file yields an empty list, not an error.
#[tauri::command]
pub fn list_workspace_packages(ws: State<'_, WsState>, id: String) -> Result<Vec<String>, String> {
    let w = ws.get(&id)?;
    let mut names = read_workspace_packages(&PathBuf::from(&w.path));
    names.sort_by_key(|name| name.to_lowercase());
    Ok(names)
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

/// Create a GitHub repository straight from the workspace and wire it as `origin`.
/// Runs `gh repo create <name> [--private|--public] --source <dir> --remote origin [--push]`,
/// so a fresh workspace goes from "no remote" to "pushed to GitHub" in one action.
#[tauri::command]
pub fn create_github_repo(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    id: String,
    repo_name: String,
    private: bool,
    push: bool,
) -> Result<String, String> {
    let w = ws.get(&id)?;
    let repo_name = repo_name.trim().to_string();
    if repo_name.is_empty() || repo_name.contains(' ') {
        return Err("Enter a repository name without spaces (owner/name is allowed).".to_string());
    }
    let dir = PathBuf::from(&w.path);
    if !dir.is_dir() {
        return Err(format!("Workspace folder is missing: {}", w.path));
    }
    if git::remote_url(&dir).is_some() {
        return Err("This workspace already has a git remote — use Push to remote instead.".to_string());
    }

    let job_id = jobs.create_job(&app, "github-create-repo", None, format!("gh repo create {repo_name}"));
    let st = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let jid = job_id.clone();
    let ws_id = w.id.clone();
    let path = w.path.clone();

    std::thread::spawn(move || {
        // Creating + wiring the repo is quick; the optional push is the unsafe phase, so
        // keep the whole job non-cancellable to avoid a half-created remote.
        if !st.mark_running_phase(&app, &jid, "creating GitHub repository", false) {
            return;
        }
        let visibility = if private { "--private" } else { "--public" };
        let mut args: Vec<String> = vec![
            "repo".into(),
            "create".into(),
            repo_name,
            visibility.into(),
            "--source".into(),
            path,
            "--remote".into(),
            "origin".into(),
        ];
        if push {
            args.push("--push".into());
        }
        match st.stream_process(&app, &jid, "gh", &args, Some(&dir), &[]) {
            Ok(0) => {
                st.log(&app, &jid, "✓ GitHub repository created and set as origin.");
                if push {
                    st.log(&app, &jid, "✓ Pushed the initial commit.");
                    wstate.modify(|list| {
                        if let Some(entry) = list.iter_mut().find(|x| x.id == ws_id) {
                            entry.last_push = Some(now_ms());
                        }
                    });
                }
                let _ = app.emit("workspaces-changed", ());
                st.finish(&app, &jid, Some(0));
            }
            Ok(code) => {
                st.log(&app, &jid, "✗ gh repo create failed. If this is an auth error, sign in on Settings → GitHub first. If the name is taken, pick another.");
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

/// Deploy a Creatio workspace straight from a GitHub repository into an
/// environment — e.g. to restore a broken dev environment from known-good
/// source, or move a repo's packages onto a fresh environment. Clones (or
/// hard-refreshes) the repo at `branch`, verifies it is a clio workspace, then
/// runs `push-workspace` to install it. Optionally keeps the clone as a
/// registered workspace so you can keep iterating on it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn deploy_from_github(
    app: AppHandle,
    jobs: State<'_, JobState>,
    ws: State<'_, WsState>,
    repo: String,
    clone_url: String,
    branch: String,
    dest_parent: String,
    target_env: String,
    skip_backup: bool,
    register: bool,
) -> Result<String, String> {
    let repo = repo.trim().to_string();
    let branch = branch.trim().to_string();
    let target_env = target_env.trim().to_string();
    if repo.is_empty() || !repo.contains('/') {
        return Err("Choose a GitHub repository (owner/name).".to_string());
    }
    if branch.is_empty() {
        return Err("Choose a branch to deploy.".to_string());
    }
    if target_env.is_empty() {
        return Err("Choose a target environment.".to_string());
    }
    let leaf = repo.rsplit('/').next().unwrap_or("repo").to_string();
    let parent = PathBuf::from(dest_parent.trim());
    if !parent.is_dir() {
        return Err(format!("Destination folder does not exist: {}", parent.display()));
    }
    let dir = parent.join(&leaf);

    let job_id = jobs.create_job(
        &app,
        "deploy-from-github",
        Some(target_env.clone()),
        format!("deploy {repo}@{branch} → {target_env}"),
    );
    let lock = jobs.env_lock(Some(&target_env));
    let st = jobs.inner().clone();
    let wstate = ws.inner().clone();
    let jid = job_id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !st.mark_running_phase(&app, &jid, "fetching from GitHub", true) {
            return;
        }

        let result = (|| -> Result<i32, String> {
            // 1. Bring the local copy to the exact branch tip.
            if dir.join(".git").is_dir() {
                st.log(&app, &jid, format!("Refreshing existing clone at {}…", dir.display()));
                git::git_ok(&dir, &["fetch", "--prune", "origin"])?;
                git::git_ok(&dir, &["checkout", &branch])?;
                git::git_ok(&dir, &["reset", "--hard", &format!("origin/{branch}")])?;
            } else if dir.exists()
                && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true)
            {
                return Err(format!(
                    "{} already exists and is not a git clone. Pick another destination.",
                    dir.display()
                ));
            } else {
                st.log(&app, &jid, format!("Cloning {repo} ({branch})…"));
                // Prefer gh (uses the signed-in account for private repos); fall back to git.
                let gh_args: Vec<String> = vec![
                    "repo".into(),
                    "clone".into(),
                    repo.clone(),
                    dir.to_string_lossy().to_string(),
                    "--".into(),
                    "-b".into(),
                    branch.clone(),
                ];
                let code = st.stream_process(&app, &jid, "gh", &gh_args, Some(&parent), &[])?;
                if code != 0 {
                    st.log(&app, &jid, "gh clone failed — falling back to git clone…");
                    git::git_ok(
                        &parent,
                        &["clone", "-b", &branch, clone_url.trim(), &leaf],
                    )?;
                }
            }

            // 2. Only clio workspaces can be installed.
            if !dir.join(".clio").is_dir() {
                return Err(format!(
                    "{repo} is not a clio workspace (no .clio directory). Only DevHub/clio workspaces can be deployed."
                ));
            }

            if st.is_cancel_requested(&jid) {
                return Err("Cancelled before install.".to_string());
            }
            // 3. Install into the environment (unsafe phase — server compile).
            if !st.set_phase(&app, &jid, "installing workspace in environment", false) {
                return Err("Cancelled before install.".to_string());
            }
            st.log(&app, &jid, "Packing and installing the workspace — the server-side compile can take several minutes and cannot be safely cancelled once it starts.");
            let mut args: Vec<String> = vec!["push-workspace".into(), "-e".into(), target_env.clone()];
            if skip_backup {
                args.push("--skip-backup".into());
                args.push("true".into());
            }
            st.stream_process(&app, &jid, "clio", &args, Some(&dir), &[])
        })();

        match result {
            Ok(0) => {
                st.log(&app, &jid, format!("✓ Deployed {repo}@{branch} to {target_env}."));
                if register {
                    let snapshot = clio::packages_snapshot(&target_env).ok();
                    let path = dir.to_string_lossy().to_string();
                    wstate.modify(|list| {
                        if !list.iter().any(|x| x.path.eq_ignore_ascii_case(&path)) {
                            list.push(Workspace {
                                id: format!("ws-{}", now_ms()),
                                name: leaf.clone(),
                                path: path.clone(),
                                env: target_env.clone(),
                                app_code: None,
                                created_at: now_ms(),
                                last_pull: Some(now_ms()),
                                last_push: Some(now_ms()),
                                packages_snapshot: snapshot.clone(),
                            });
                        }
                    });
                    let _ = app.emit("workspaces-changed", ());
                }
                st.finish(&app, &jid, Some(0));
            }
            Ok(code) => {
                st.log(&app, &jid, "✗ Install failed — scroll up for the compile/installation errors. The environment keeps its backup unless you skipped it.");
                st.finish(&app, &jid, Some(code));
            }
            Err(e) => {
                st.log(&app, &jid, format!("✗ {e}"));
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

    #[test]
    fn reads_workspace_packages_from_settings() {
        let dir = std::env::temp_dir().join(format!(
            "devhub-workspace-packages-test-{}",
            std::process::id()
        ));
        let clio_dir = dir.join(".clio");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&clio_dir).unwrap();
        std::fs::write(
            clio_dir.join("workspaceSettings.json"),
            r#"{"Packages":["QntImportEngine","Base"],"IgnorePackages":[]}"#,
        )
        .unwrap();

        let packages = read_workspace_packages(&dir);
        assert_eq!(packages, vec!["QntImportEngine", "Base"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_no_packages_when_settings_missing() {
        let dir = std::env::temp_dir().join(format!(
            "devhub-workspace-packages-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_workspace_packages(&dir).is_empty());
    }
}
