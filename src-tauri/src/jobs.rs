use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub id: String,
    pub kind: String,
    pub env: Option<String>,
    /// Full CLI invocation with secret values masked — safe to display and log.
    pub display_command: String,
    pub status: String, // queued | running | cancelling | cancelled | succeeded | failed
    pub phase: String,
    pub cancellable: bool,
    pub cancel_requested: bool,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
    /// Cause and resolution for a failed job, when the log matches a known
    /// failure. Absent on success and on failures DevHub does not recognize.
    #[serde(default)]
    pub diagnosis: Option<crate::diagnostics::Diagnosis>,
    /// Background work the user did not personally start — health checks and
    /// similar. It still appears in Jobs with full output, but it raises no
    /// toast and no desktop notification, so opening the app with several
    /// environments registered does not produce a burst of alerts.
    #[serde(default)]
    pub quiet: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLogLine {
    pub id: String,
    pub line: String,
}

#[derive(Default, Clone)]
pub struct JobState {
    pub jobs: Arc<Mutex<Vec<JobInfo>>>,
    pub logs: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// One lock per environment so jobs against the same env run sequentially.
    pub env_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    pub process_ids: Arc<Mutex<HashMap<String, u32>>>,
    /// Set once at startup; None only in unit tests that don't touch persistence.
    pub store: Arc<Mutex<Option<JobStore>>>,
}

/// Persistent job history under the app-data dir:
/// `jobs/history.json` (newest first, capped) + `jobs/logs/{id}.log` per finished job.
const MAX_HISTORY: usize = 200;

#[derive(Clone)]
pub struct JobStore {
    dir: PathBuf,
}

impl JobStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(dir.join("logs"));
        JobStore { dir }
    }

    fn history_file(&self) -> PathBuf {
        self.dir.join("history.json")
    }

    fn log_file(&self, id: &str) -> PathBuf {
        self.dir.join("logs").join(format!("{id}.log"))
    }

    /// Load past jobs; anything recorded as still active was orphaned by an app
    /// exit and is surfaced as failed/interrupted rather than silently dropped.
    pub fn load(&self) -> Vec<JobInfo> {
        let mut list: Vec<JobInfo> = std::fs::read_to_string(self.history_file())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        for j in &mut list {
            if !matches!(j.status.as_str(), "succeeded" | "failed" | "cancelled") {
                j.status = "failed".to_string();
                j.phase = "interrupted — DevHub closed while the job was active".to_string();
                j.cancellable = false;
                j.cancel_requested = false;
                if j.finished_at.is_none() {
                    j.finished_at = Some(j.started_at);
                }
            }
        }
        list
    }

    pub fn persist(&self, jobs: &[JobInfo]) {
        let capped: Vec<&JobInfo> = jobs.iter().take(MAX_HISTORY).collect();
        if let Ok(json) = serde_json::to_string_pretty(&capped) {
            let _ = std::fs::write(self.history_file(), json);
        }
        for pruned in jobs.iter().skip(MAX_HISTORY) {
            let _ = std::fs::remove_file(self.log_file(&pruned.id));
        }
    }

    pub fn write_log(&self, id: &str, lines: &[String]) {
        let _ = std::fs::write(self.log_file(id), lines.join("\n"));
    }

    pub fn read_log(&self, id: &str) -> Option<Vec<String>> {
        std::fs::read_to_string(self.log_file(id))
            .ok()
            .map(|s| s.lines().map(String::from).collect())
    }

    pub fn remove_log(&self, id: &str) {
        let _ = std::fs::remove_file(self.log_file(id));
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn scrub(line: &str, secrets: &[String]) -> String {
    let mut out = line.to_string();
    for s in secrets {
        if !s.is_empty() {
            out = out.replace(s.as_str(), "•••");
        }
    }
    out
}

/// Options whose following value must never be shown.
const SECRET_FLAGS: [&str; 4] = ["-p", "--Password", "--password", "--ClientSecret"];

fn display_command(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    let mut mask_next = false;
    for a in args {
        if mask_next {
            parts.push("•••".to_string());
            mask_next = false;
            continue;
        }
        if SECRET_FLAGS.contains(&a.as_str()) {
            mask_next = true;
        }
        parts.push(a.clone());
    }
    parts.join(" ")
}

fn secret_values(args: &[String]) -> Vec<String> {
    let mut secrets = Vec::new();
    let mut take_next = false;
    for a in args {
        if take_next {
            secrets.push(a.clone());
            take_next = false;
            continue;
        }
        if SECRET_FLAGS.contains(&a.as_str()) {
            take_next = true;
        }
    }
    secrets
}

impl JobState {
    /// Called once from setup: attach the store and surface past jobs.
    pub fn init_persistence(&self, dir: PathBuf) {
        let store = JobStore::new(dir);
        let loaded = store.load();
        {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.extend(loaded);
        }
        store.persist(&self.jobs.lock().unwrap());
        *self.store.lock().unwrap() = Some(store);
    }

    fn persist_snapshot(&self) {
        let store = self.store.lock().unwrap().clone();
        if let Some(store) = store {
            let snapshot = self.jobs.lock().unwrap().clone();
            store.persist(&snapshot);
        }
    }

    pub fn env_lock(&self, env: Option<&str>) -> Arc<Mutex<()>> {
        let mut locks = self.env_locks.lock().unwrap();
        locks
            .entry(env.unwrap_or("_global").to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn create_job(&self, app: &AppHandle, kind: &str, env: Option<String>, display: String) -> String {
        self.create_job_with(app, kind, env, display, false)
    }

    /// `quiet` marks background work that must not raise toasts or desktop
    /// notifications. See `JobInfo::quiet`.
    pub fn create_job_with(
        &self,
        app: &AppHandle,
        kind: &str,
        env: Option<String>,
        display: String,
        quiet: bool,
    ) -> String {
        let id = format!("{}-{}", now_ms(), kind);
        let job = JobInfo {
            quiet,
            id: id.clone(),
            kind: kind.to_string(),
            env,
            display_command: display,
            status: "queued".to_string(),
            phase: "waiting".to_string(),
            cancellable: true,
            cancel_requested: false,
            started_at: now_ms(),
            finished_at: None,
            exit_code: None,
            diagnosis: None,
        };
        self.jobs.lock().unwrap().insert(0, job.clone());
        self.logs.lock().unwrap().insert(id.clone(), Vec::new());
        let _ = app.emit("job-update", job);
        self.persist_snapshot();
        id
    }

    pub fn update(&self, app: &AppHandle, id: &str, f: impl FnOnce(&mut JobInfo)) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            f(job);
            let _ = app.emit("job-update", job.clone());
        }
    }

    pub fn log(&self, app: &AppHandle, id: &str, line: impl Into<String>) {
        let line = line.into();
        self.logs.lock().unwrap().entry(id.to_string()).or_default().push(line.clone());
        let _ = app.emit("job-log", JobLogLine { id: id.to_string(), line });
    }

    pub fn mark_running_phase(
        &self,
        app: &AppHandle,
        id: &str,
        phase: &str,
        cancellable: bool,
    ) -> bool {
        let mut jobs = self.jobs.lock().unwrap();
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        if job.cancel_requested || job.status == "cancelled" {
            job.status = "cancelled".to_string();
            job.phase = "cancelled before start".to_string();
            job.cancellable = false;
            job.finished_at = Some(now_ms());
            let _ = app.emit("job-update", job.clone());
            return false;
        }
        job.status = "running".to_string();
        job.phase = phase.to_string();
        job.cancellable = cancellable;
        let _ = app.emit("job-update", job.clone());
        true
    }

    pub fn set_phase(
        &self,
        app: &AppHandle,
        id: &str,
        phase: &str,
        cancellable: bool,
    ) -> bool {
        let mut jobs = self.jobs.lock().unwrap();
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        if job.cancel_requested {
            return false;
        }
        job.phase = phase.to_string();
        job.cancellable = cancellable;
        let _ = app.emit("job-update", job.clone());
        true
    }

    pub fn is_cancel_requested(&self, id: &str) -> bool {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .find(|job| job.id == id)
            .is_some_and(|job| job.cancel_requested)
    }

    pub fn finish(&self, app: &AppHandle, id: &str, code: Option<i32>) {
        // Explain the failure while the log is still in memory. A zero exit is not
        // taken at face value: clio can exit 0 after Creatio answered with a 500,
        // so the log gets the final word on whether the job succeeded.
        let (diagnosis, failed) = {
            let logs = self.logs.lock().unwrap();
            let lines = logs.get(id);
            if code == Some(0) {
                let diagnosis = lines.and_then(|l| crate::diagnostics::failure_despite_zero_exit(l));
                let failed = diagnosis.is_some();
                (diagnosis, failed)
            } else {
                (lines.and_then(|l| crate::diagnostics::diagnose_log(l)), true)
            }
        };
        if failed && code == Some(0) {
            self.log(
                app,
                id,
                "[DevHub] The tool exited 0, but its output reports a server error — \
                 this job is marked failed. Verify the target environment before retrying."
                    .to_string(),
            );
        }
        self.update(app, id, |j| {
            j.diagnosis = diagnosis.clone();
            j.exit_code = code;
            j.finished_at = Some(now_ms());
            j.cancellable = false;
            if j.cancel_requested {
                j.status = "cancelled".to_string();
                j.phase = "cancelled".to_string();
            } else {
                j.status = if failed { "failed".to_string() } else { "succeeded".to_string() };
                j.phase = if failed { "failed".to_string() } else { "completed".to_string() };
            }
        });
        // Persist the terminal state and dump this job's log to its file.
        {
            let store = self.store.lock().unwrap().clone();
            if let Some(store) = store {
                if let Some(lines) = self.logs.lock().unwrap().get(id) {
                    store.write_log(id, lines);
                }
            }
        }
        self.persist_snapshot();
        // Background work never notifies, however it ends.
        if self.jobs.lock().unwrap().iter().any(|j| j.id == id && j.quiet) {
            return;
        }
        // Long jobs finish while the user is elsewhere — notify unless the window is focused.
        let focused = tauri::Manager::get_webview_window(app, "main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        if !focused {
            if let Some(job) = self.jobs.lock().unwrap().iter().find(|j| j.id == id) {
                use tauri_plugin_notification::NotificationExt;
                let ok = job.status == "succeeded";
                let cancelled = job.status == "cancelled";
                let _ = app
                    .notification()
                    .builder()
                    .title(if cancelled {
                        "Job cancelled"
                    } else if ok {
                        "Job finished"
                    } else {
                        "Job failed"
                    })
                    .body(format!(
                        "{}{} — {}",
                        job.kind,
                        job.env.as_ref().map(|e| format!(" ({e})")).unwrap_or_default(),
                        if cancelled {
                            "stopped before an unsafe phase"
                        } else if ok {
                            "completed successfully"
                        } else {
                            "check the Jobs screen"
                        }
                    ))
                    .show();
            }
        }
    }

    /// Run a process to completion, streaming every output line into the job log.
    /// Returns the exit code.
    pub fn stream_process(
        &self,
        app: &AppHandle,
        id: &str,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        secrets: &[String],
    ) -> Result<i32, String> {
        self.log(app, id, format!("$ {}", display_command(program, args)));
        let mut cmd = Command::new(crate::tools::resolve(program));
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
        // Never let git block on an interactive credential prompt.
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start {program}: {e}. {}", crate::tools::not_found(program)))?;
        self.process_ids.lock().unwrap().insert(id.to_string(), child.id());
        if self.is_cancel_requested(id) {
            terminate_process_tree(child.id());
        }

        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(self.spawn_reader(out, app.clone(), id.to_string(), secrets.to_vec()));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(self.spawn_reader(err, app.clone(), id.to_string(), secrets.to_vec()));
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        self.process_ids.lock().unwrap().remove(id);
        for r in readers {
            let _ = r.join();
        }
        Ok(status.code().unwrap_or(-1))
    }

    fn spawn_reader<R: std::io::Read + Send + 'static>(
        &self,
        stream: R,
        app: AppHandle,
        job_id: String,
        secrets: Vec<String>,
    ) -> std::thread::JoinHandle<()> {
        let logs = self.logs.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                let clean = scrub(&line, &secrets);
                logs.lock().unwrap().entry(job_id.clone()).or_default().push(clean.clone());
                let _ = app.emit("job-log", JobLogLine { id: job_id.clone(), line: clean });
            }
        })
    }
}

/// Launch `clio <args>` as a tracked job. Returns the job id immediately;
/// progress arrives via `job-update` and `job-log` events.
#[tauri::command]
pub fn run_clio_job(
    app: AppHandle,
    state: State<'_, JobState>,
    kind: String,
    args: Vec<String>,
    env: Option<String>,
    cwd: Option<String>,
    quiet: Option<bool>,
) -> Result<String, String> {
    let secrets = secret_values(&args);
    let id = state.create_job_with(
        &app,
        &kind,
        env.clone(),
        display_command("clio", &args),
        quiet.unwrap_or(false),
    );
    let lock = state.env_lock(env.as_deref());
    let st = state.inner().clone();
    let job_id = id.clone();

    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        let cancellable = matches!(kind.as_str(), "ping-app" | "open-web-app" | "reg-web-app");
        let phase =
            if kind == "install-gate" { "installing cliogate" } else { "running clio command" };
        if !st.mark_running_phase(&app, &job_id, phase, cancellable) {
            return;
        }
        let dir = cwd.map(std::path::PathBuf::from);
        match st.stream_process(&app, &job_id, "clio", &args, dir.as_deref(), &secrets) {
            Ok(code) => st.finish(&app, &job_id, Some(code)),
            Err(e) => {
                st.log(&app, &job_id, e);
                st.finish(&app, &job_id, None);
            }
        }
    });

    Ok(id)
}

#[tauri::command]
pub fn get_jobs(state: State<'_, JobState>) -> Vec<JobInfo> {
    state.jobs.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_job_log(state: State<'_, JobState>, id: String) -> Vec<String> {
    if let Some(lines) = state.logs.lock().unwrap().get(&id) {
        if !lines.is_empty() {
            return lines.clone();
        }
    }
    // Not in memory (job from a previous run) — read its persisted log file.
    let store = state.store.lock().unwrap().clone();
    store.and_then(|s| s.read_log(&id)).unwrap_or_default()
}

/// Remove finished jobs (and their log files) from history; active jobs stay.
#[tauri::command]
pub fn clear_job_history(state: State<'_, JobState>) -> Vec<JobInfo> {
    let removed: Vec<JobInfo>;
    {
        let mut jobs = state.jobs.lock().unwrap();
        let (keep, drop): (Vec<JobInfo>, Vec<JobInfo>) = jobs
            .drain(..)
            .partition(|j| !matches!(j.status.as_str(), "succeeded" | "failed" | "cancelled"));
        *jobs = keep;
        removed = drop;
    }
    let store = state.store.lock().unwrap().clone();
    if let Some(store) = store {
        for j in &removed {
            store.remove_log(&j.id);
        }
    }
    {
        let mut logs = state.logs.lock().unwrap();
        for j in &removed {
            logs.remove(&j.id);
        }
    }
    state.persist_snapshot();
    state.jobs.lock().unwrap().clone()
}

fn terminate_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
        let _ = command.status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}

#[tauri::command]
pub fn cancel_job(app: AppHandle, state: State<'_, JobState>, id: String) -> Result<(), String> {
    let pid;
    {
        let mut jobs = state.jobs.lock().unwrap();
        let job = jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| "Job not found.".to_string())?;
        if matches!(job.status.as_str(), "succeeded" | "failed" | "cancelled") {
            return Err("This job has already finished.".to_string());
        }
        if !job.cancellable {
            return Err(format!(
                "This job cannot be stopped safely during the '{}' phase.",
                job.phase
            ));
        }
        job.cancel_requested = true;
        if job.status == "queued" {
            job.status = "cancelled".to_string();
            job.phase = "cancelled before start".to_string();
            job.cancellable = false;
            job.finished_at = Some(now_ms());
        } else {
            job.status = "cancelling".to_string();
            job.phase = format!("stopping {}", job.phase);
        }
        let _ = app.emit("job-update", job.clone());
        pid = state.process_ids.lock().unwrap().get(&id).copied();
    }
    state.log(
        &app,
        &id,
        "Cancellation requested. Stopping the local process tree…",
    );
    if let Some(pid) = pid {
        terminate_process_tree(pid);
    }
    state.persist_snapshot();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_job(id: &str, status: &str) -> JobInfo {
        JobInfo {
            id: id.to_string(),
            kind: "test".to_string(),
            env: Some("env1".to_string()),
            display_command: "clio test".to_string(),
            status: status.to_string(),
            phase: "running clio command".to_string(),
            cancellable: false,
            cancel_requested: false,
            started_at: 1,
            finished_at: if status == "succeeded" { Some(2) } else { None },
            exit_code: if status == "succeeded" { Some(0) } else { None },
            diagnosis: None,
            quiet: false,
        }
    }

    #[test]
    fn job_history_written_before_quiet_existed_still_loads() {
        // history.json files from earlier versions have neither field; both must
        // default rather than failing the whole history load.
        let old = r#"{
            "id": "1-test", "kind": "deploy", "env": "dev-834",
            "displayCommand": "clio push-pkg X", "status": "succeeded",
            "phase": "completed", "cancellable": false, "cancelRequested": false,
            "startedAt": 1, "finishedAt": 2, "exitCode": 0
        }"#;
        let job: JobInfo = serde_json::from_str(old).expect("legacy record should load");
        assert!(!job.quiet);
        assert!(job.diagnosis.is_none());
        assert_eq!(job.status, "succeeded");
    }

    #[test]
    fn history_roundtrip_marks_orphaned_jobs_interrupted() {
        let dir = std::env::temp_dir().join(format!("devhub-jobs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = JobStore::new(dir.clone());

        let jobs = vec![sample_job("j-running", "running"), sample_job("j-done", "succeeded")];
        store.persist(&jobs);
        store.write_log("j-done", &["line one".to_string(), "line two".to_string()]);

        let loaded = store.load();
        assert_eq!(loaded.len(), 2);
        let orphan = loaded.iter().find(|j| j.id == "j-running").unwrap();
        assert_eq!(orphan.status, "failed");
        assert!(orphan.phase.contains("interrupted"));
        assert!(orphan.finished_at.is_some());
        let done = loaded.iter().find(|j| j.id == "j-done").unwrap();
        assert_eq!(done.status, "succeeded");
        assert_eq!(store.read_log("j-done").unwrap(), vec!["line one", "line two"]);

        store.remove_log("j-done");
        assert!(store.read_log("j-done").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_caps_and_prunes_logs() {
        let dir = std::env::temp_dir().join(format!("devhub-jobs-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = JobStore::new(dir.clone());

        let jobs: Vec<JobInfo> =
            (0..MAX_HISTORY + 5).map(|i| sample_job(&format!("j{i}"), "succeeded")).collect();
        for j in &jobs {
            store.write_log(&j.id, &["x".to_string()]);
        }
        store.persist(&jobs);

        let loaded = store.load();
        assert_eq!(loaded.len(), MAX_HISTORY);
        // logs of pruned (beyond-cap) jobs are deleted, capped ones remain
        assert!(store.read_log(&format!("j{}", MAX_HISTORY + 1)).is_none());
        assert!(store.read_log("j0").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(windows)]
    fn terminates_a_running_process_tree() {
        use std::os::windows::process::CommandExt;
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        terminate_process_tree(child.id());
        let status = child.wait().unwrap();
        assert!(!status.success());
    }
}
