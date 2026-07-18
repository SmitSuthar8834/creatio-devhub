use std::path::Path;
use std::process::{Command, Stdio};

/// Run git synchronously in `cwd`, returning (exit_code, stdout, stderr).
/// GIT_TERMINAL_PROMPT=0 turns would-be credential prompts into fast failures.
pub fn git(cwd: &Path, args: &[&str]) -> Result<(i32, String, String), String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("Failed to start git: {e}. Is git installed and on PATH?"))?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    ))
}

/// Like `git`, but errors when the command exits non-zero.
pub fn git_ok(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let (code, stdout, stderr) = git(cwd, args)?;
    if code != 0 {
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(format!("git {} failed: {}", args.join(" "), detail.trim()));
    }
    Ok(stdout)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub status: String, // M | A | D | R | ? …
}

/// Parse `git status --porcelain` into per-file changes (untracked included).
pub fn status(cwd: &Path) -> Result<Vec<FileChange>, String> {
    let out = git_ok(cwd, &["status", "--porcelain"])?;
    let mut changes = Vec::new();
    for line in out.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].trim().trim_matches('"').to_string();
        let status = if code.starts_with("??") {
            "A".to_string() // untracked → will be added
        } else {
            code.trim().chars().next().unwrap_or('M').to_string()
        };
        changes.push(FileChange { path, status });
    }
    Ok(changes)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

pub fn log(cwd: &Path, limit: u32) -> Result<Vec<Commit>, String> {
    let format = "--pretty=format:%h\x1f%s\x1f%an\x1f%ad";
    let n = format!("-{limit}");
    let (code, out, _) = git(cwd, &["log", &n, "--date=relative", format])?;
    if code != 0 {
        return Ok(vec![]); // no commits yet
    }
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut p = l.split('\x1f');
            Some(Commit {
                hash: p.next()?.to_string(),
                message: p.next()?.to_string(),
                author: p.next()?.to_string(),
                date: p.next()?.to_string(),
            })
        })
        .collect())
}

pub fn current_branch(cwd: &Path) -> Option<String> {
    git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|(code, _, _)| *code == 0)
        .map(|(_, out, _)| out.trim().to_string())
}

pub fn remote_url(cwd: &Path) -> Option<String> {
    git(cwd, &["remote", "get-url", "origin"])
        .ok()
        .filter(|(code, _, _)| *code == 0)
        .map(|(_, out, _)| out.trim().to_string())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub has_remote: bool,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

fn rev_count(cwd: &Path, range: &str) -> u32 {
    git(cwd, &["rev-list", "--count", range])
        .ok()
        .filter(|(code, _, _)| *code == 0)
        .and_then(|(_, output, _)| output.trim().parse().ok())
        .unwrap_or(0)
}

/// Fetch origin and compare the current branch with its remote counterpart.
/// A non-zero `behind` count means another contributor pushed commits that the
/// local workspace does not have and a push must be blocked.
pub fn remote_status(cwd: &Path) -> Result<RemoteStatus, String> {
    if remote_url(cwd).is_none() {
        return Ok(RemoteStatus {
            has_remote: false,
            branch: current_branch(cwd),
            ahead: 0,
            behind: 0,
        });
    }
    if let Err(error) = git_ok(cwd, &["fetch", "--prune", "origin"]) {
        if error.contains("Repository not found") {
            let account = Command::new("gh")
                .args(["api", "user", "--jq", ".login"])
                .stdin(Stdio::null())
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|login| !login.is_empty());
            let remote = remote_url(cwd).unwrap_or_else(|| "the configured origin".to_string());
            return Err(match account {
                Some(account) => format!(
                    "The active GitHub account '{account}' cannot access the private repository {remote}. Switch to an account with access, or add '{account}' as a repository collaborator and accept the invitation."
                ),
                None => format!(
                    "The authenticated GitHub account cannot access the private repository {remote}. Sign in with an account that has access."
                ),
            });
        }
        return Err(error);
    }
    let branch = current_branch(cwd);
    let Some(branch_name) = branch.as_deref() else {
        return Ok(RemoteStatus {
            has_remote: true,
            branch,
            ahead: 0,
            behind: 0,
        });
    };
    let upstream = format!("origin/{branch_name}");
    let exists = git(cwd, &["rev-parse", "--verify", "--quiet", &upstream])
        .map(|(code, _, _)| code == 0)
        .unwrap_or(false);
    if !exists {
        return Ok(RemoteStatus {
            has_remote: true,
            branch,
            ahead: 0,
            behind: 0,
        });
    }
    Ok(RemoteStatus {
        has_remote: true,
        branch,
        ahead: rev_count(cwd, &format!("{upstream}..HEAD")),
        behind: rev_count(cwd, &format!("HEAD..{upstream}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_status_commit_log_roundtrip() {
        let dir = std::env::temp_dir().join(format!("devhub-git-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        git_ok(&dir, &["init", "-b", "main"]).unwrap();
        std::fs::write(dir.join("Schema.cs"), "public class A {}").unwrap();

        let changes = status(&dir).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "Schema.cs");
        assert_eq!(changes[0].status, "A");

        git_ok(&dir, &["add", "-A"]).unwrap();
        git_ok(&dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-m", "initial"]).unwrap();

        assert_eq!(status(&dir).unwrap().len(), 0);
        let commits = log(&dir, 10).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "initial");
        assert_eq!(current_branch(&dir).as_deref(), Some("main"));
        assert_eq!(remote_url(&dir), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_status_detects_commits_pushed_by_another_clone() {
        let root =
            std::env::temp_dir().join(format!("devhub-remote-test-{}", std::process::id()));
        let remote = root.join("remote.git");
        let first = root.join("first");
        let second = root.join("second");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        git_ok(&root, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
        std::fs::create_dir_all(&first).unwrap();
        git_ok(&first, &["init", "-b", "main"]).unwrap();
        std::fs::write(first.join("one.txt"), "one").unwrap();
        git_ok(&first, &["add", "-A"]).unwrap();
        git_ok(
            &first,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-m",
                "one",
            ],
        )
        .unwrap();
        git_ok(
            &first,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        git_ok(&first, &["push", "-u", "origin", "main"]).unwrap();
        git_ok(
            &root,
            &[
                "clone",
                "--branch",
                "main",
                remote.to_str().unwrap(),
                second.to_str().unwrap(),
            ],
        )
        .unwrap();

        std::fs::write(first.join("two.txt"), "two").unwrap();
        git_ok(&first, &["add", "-A"]).unwrap();
        git_ok(
            &first,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-m",
                "two",
            ],
        )
        .unwrap();
        git_ok(&first, &["push", "origin", "main"]).unwrap();

        let status = remote_status(&second).unwrap();
        assert_eq!(status.behind, 1);
        assert_eq!(status.ahead, 0);
        let _ = std::fs::remove_dir_all(&root);
    }
}
