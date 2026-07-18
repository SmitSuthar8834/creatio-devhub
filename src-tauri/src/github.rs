use crate::jobs::JobState;
use serde::Serialize;
use std::process::{Command, Stdio};
use tauri::{AppHandle, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    pub gh_installed: bool,
    pub authenticated: bool,
    pub login: Option<String>,
    pub account_name: Option<String>,
    pub account_email: Option<String>,
    pub suggested_email: Option<String>,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
}

fn capture(program: &str, args: &[&str]) -> Result<(i32, String, String), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn git_config(key: &str) -> Option<String> {
    capture("git", &["config", "--global", "--get", key])
        .ok()
        .filter(|(code, value, _)| *code == 0 && !value.is_empty())
        .map(|(_, value, _)| value)
}

#[tauri::command]
pub fn github_status() -> GithubStatus {
    let git_name = git_config("user.name");
    let git_email = git_config("user.email");
    let Ok((code, output, _)) = capture("gh", &["api", "user"]) else {
        return GithubStatus {
            gh_installed: false,
            authenticated: false,
            login: None,
            account_name: None,
            account_email: None,
            suggested_email: None,
            git_name,
            git_email,
        };
    };
    if code != 0 {
        return GithubStatus {
            gh_installed: true,
            authenticated: false,
            login: None,
            account_name: None,
            account_email: None,
            suggested_email: None,
            git_name,
            git_email,
        };
    }
    let json: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();
    let login = json.get("login").and_then(|value| value.as_str()).map(str::to_string);
    let id = json.get("id").and_then(|value| value.as_u64());
    let account_email = json
        .get("email")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let suggested_email = account_email.clone().or_else(|| {
        Some(format!(
            "{}+{}@users.noreply.github.com",
            id?,
            login.as_deref()?
        ))
    });
    GithubStatus {
        gh_installed: true,
        authenticated: true,
        login,
        account_name: json
            .get("name")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        account_email,
        suggested_email,
        git_name,
        git_email,
    }
}

#[tauri::command]
pub fn set_git_identity(name: String, email: String) -> Result<(), String> {
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() || email.is_empty() || !email.contains('@') {
        return Err("Enter a valid Git name and email.".to_string());
    }
    let (code, _, error) = capture("git", &["config", "--global", "user.name", name])?;
    if code != 0 {
        return Err(format!("Could not set Git user.name: {error}"));
    }
    let (code, _, error) = capture("git", &["config", "--global", "user.email", email])?;
    if code != 0 {
        return Err(format!("Could not set Git user.email: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub fn start_github_login(
    app: AppHandle,
    jobs: State<'_, JobState>,
) -> Result<String, String> {
    if capture("gh", &["--version"]).is_err() {
        return Err("GitHub CLI (gh) is not installed or is not on PATH.".to_string());
    }
    let id = jobs.create_job(
        &app,
        "github-login",
        None,
        "gh auth login --hostname github.com --git-protocol https --web".to_string(),
    );
    let lock = jobs.env_lock(None);
    let state = jobs.inner().clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let _guard = lock.lock().unwrap();
        if !state.mark_running_phase(&app, &job_id, "waiting for GitHub sign-in", true) {
            return;
        }
        let args = vec![
            "auth".into(),
            "login".into(),
            "--hostname".into(),
            "github.com".into(),
            "--git-protocol".into(),
            "https".into(),
            "--web".into(),
            "--skip-ssh-key".into(),
        ];
        match state.stream_process(&app, &job_id, "gh", &args, None, &[]) {
            Ok(0) => {
                if state.is_cancel_requested(&job_id) {
                    state.finish(&app, &job_id, Some(1));
                    return;
                }
                if !state.set_phase(&app, &job_id, "configuring Git credentials", false) {
                    state.finish(&app, &job_id, Some(1));
                    return;
                }
                let setup = vec!["auth".into(), "setup-git".into()];
                match state.stream_process(&app, &job_id, "gh", &setup, None, &[]) {
                    Ok(code) => state.finish(&app, &job_id, Some(code)),
                    Err(error) => {
                        state.log(&app, &job_id, error);
                        state.finish(&app, &job_id, Some(1));
                    }
                }
            }
            Ok(code) => state.finish(&app, &job_id, Some(code)),
            Err(error) => {
                state.log(&app, &job_id, error);
                state.finish(&app, &job_id, Some(1));
            }
        }
    });
    Ok(id)
}
