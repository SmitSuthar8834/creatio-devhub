use crate::jobs::JobState;
use serde::Serialize;
use std::process::{Command, Stdio};
use tauri::{AppHandle, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    pub gh_installed: bool,
    /// Where gh was resolved — shown in Settings so a wrong copy is obvious.
    pub gh_path: Option<String>,
    /// Directories checked, for the "not found" message.
    pub gh_searched: Vec<String>,
    /// Why starting gh failed, when it did.
    pub gh_error: Option<String>,
    pub authenticated: bool,
    pub login: Option<String>,
    pub account_name: Option<String>,
    pub account_email: Option<String>,
    pub suggested_email: Option<String>,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
}

fn capture(program: &str, args: &[&str]) -> Result<(i32, String, String), String> {
    let mut command = Command::new(crate::tools::resolve(program));
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

/// Status with no GitHub account details — gh is either missing (`error`) or
/// present but not signed in.
fn unauthenticated(error: Option<String>) -> GithubStatus {
    GithubStatus {
        gh_installed: error.is_none(),
        gh_path: crate::tools::locate("gh").map(|path| path.to_string_lossy().to_string()),
        gh_searched: crate::tools::searched_locations("gh"),
        gh_error: error,
        authenticated: false,
        login: None,
        account_name: None,
        account_email: None,
        suggested_email: None,
        git_name: git_config("user.name"),
        git_email: git_config("user.email"),
    }
}

#[tauri::command]
pub fn github_status() -> GithubStatus {
    // Re-scan on every status check so a gh installed while DevHub is open is
    // picked up by the Refresh button rather than needing a restart.
    crate::tools::clear_cache();
    let git_name = git_config("user.name");
    let git_email = git_config("user.email");
    let (code, output, _) = match capture("gh", &["api", "user"]) {
        Ok(result) => result,
        Err(error) => return unauthenticated(Some(error)),
    };
    if code != 0 {
        return unauthenticated(None);
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
        gh_path: crate::tools::locate("gh").map(|path| path.to_string_lossy().to_string()),
        gh_searched: crate::tools::searched_locations("gh"),
        gh_error: None,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepo {
    pub name_with_owner: String,
    pub name: String,
    pub url: String,
    pub default_branch: String,
    pub is_private: bool,
}

/// List the signed-in account's GitHub repositories (for the "Deploy from
/// GitHub" picker). Read-only; requires `gh` to be authenticated.
#[tauri::command]
pub fn list_github_repos() -> Result<Vec<GithubRepo>, String> {
    let (code, out, err) = capture(
        "gh",
        &[
            "repo",
            "list",
            "--no-archived",
            "--limit",
            "200",
            "--json",
            "nameWithOwner,name,url,defaultBranchRef,isPrivate",
        ],
    )?;
    if code != 0 {
        return Err(if err.trim().is_empty() {
            "Could not list GitHub repositories. Sign in on Settings → GitHub first.".to_string()
        } else {
            err
        });
    }
    let json: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let rows = json.as_array().ok_or("Unexpected gh output.".to_string())?;
    let mut repos: Vec<GithubRepo> = rows
        .iter()
        .filter_map(|row| {
            let name_with_owner = row.get("nameWithOwner")?.as_str()?.to_string();
            Some(GithubRepo {
                name_with_owner,
                name: row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                url: row.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                default_branch: row
                    .get("defaultBranchRef")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("main")
                    .to_string(),
                is_private: row.get("isPrivate").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        })
        .collect();
    repos.sort_by(|a, b| a.name_with_owner.to_lowercase().cmp(&b.name_with_owner.to_lowercase()));
    Ok(repos)
}

/// List branch names of a `owner/name` repository via the GitHub API.
#[tauri::command]
pub fn list_repo_branches(repo: String) -> Result<Vec<String>, String> {
    let repo = repo.trim();
    if repo.is_empty() || !repo.contains('/') {
        return Err("Expected a repository in owner/name form.".to_string());
    }
    let path = format!("repos/{repo}/branches");
    let (code, out, err) = capture("gh", &["api", "--paginate", &path, "--jq", ".[].name"])?;
    if code != 0 {
        return Err(if err.trim().is_empty() {
            "Could not list branches for this repository.".to_string()
        } else {
            err
        });
    }
    Ok(out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
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
        return Err(crate::tools::not_found("gh"));
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
