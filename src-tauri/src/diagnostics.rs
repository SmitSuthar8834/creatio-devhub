//! Turning raw CLI output into a cause and a fix.
//!
//! DevHub shells out to clio, git, and gh, so failures arrive as whatever those
//! tools printed — stack traces, HTTP codes, NuGet tool-store paths. That text
//! is accurate but rarely actionable, and the useful line is often buried in a
//! hundred lines of log.
//!
//! This module matches known failure signatures and returns a plain-language
//! summary, the likely cause, and the steps that resolve it. Anything unmatched
//! returns None and the caller keeps showing the raw output, so an unrecognized
//! error is never *worse* than before — only unexplained.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// Stable identifier, handy for tests and telemetry.
    pub code: String,
    /// One line: what went wrong, in the user's terms.
    pub summary: String,
    /// Why it happens — the part that stops it recurring.
    pub cause: String,
    /// Ordered, concrete things to do.
    pub steps: Vec<String>,
}

struct Rule {
    code: &'static str,
    /// Every needle must be present for the rule to match.
    all: &'static [&'static str],
    /// At least one needle must be present. Empty means no extra requirement.
    any: &'static [&'static str],
    /// If any of these are present the rule is skipped — keeps rules that share
    /// vocabulary (git 403 vs. Creatio 403) from stealing each other's matches.
    none: &'static [&'static str],
    summary: &'static str,
    cause: &'static str,
    steps: &'static [&'static str],
}

/// Ordered most specific first; the first match wins.
const RULES: &[Rule] = &[
    Rule {
        code: "clio-damaged",
        all: &["could not load file or assembly"],
        any: &[],
        none: &[],
        summary: "clio's installation is incomplete.",
        cause: "A required assembly is missing from the installed clio tool, usually after an interrupted or partial update.",
        steps: &[
            "Use \"Repair clio\" in the DevHub header banner.",
            "If that fails, run: dotnet tool uninstall clio -g",
            "Then run: dotnet tool install clio -g",
        ],
    },
    Rule {
        code: "clio-locked",
        all: &[],
        any: &[
            "failed to uninstall tool package",
            "being used by another process",
            "access to the path",
        ],
        none: &[],
        summary: "clio's files are locked by another process.",
        cause: "A clio command is still running somewhere — another DevHub job, or a terminal you left open — so its files cannot be replaced.",
        steps: &[
            "Wait for running jobs to finish in the Jobs screen.",
            "Close any terminal or editor that is running clio.",
            "Retry the operation.",
            "If it still fails, restart DevHub as administrator.",
        ],
    },
    Rule {
        code: "cliogate-missing",
        all: &["cliogate"],
        any: &["install", "not found", "is not installed", "version"],
        none: &[],
        summary: "This environment needs the cliogate helper package.",
        cause: "Workspace synchronization and several package operations run through cliogate, which is installed per Creatio environment and is either missing or too old here.",
        steps: &[
            "Open the Environments screen and install cliogate for this environment.",
            "If it is already installed, update it — the server copy may predate your clio version.",
            "Retry the operation.",
        ],
    },
    Rule {
        code: "tool-missing",
        all: &[],
        any: &[
            "is not recognized",
            "no such file or directory",
            "could not find clio",
            "could not find gh",
            "could not find git",
            "could not find dotnet",
            "the system cannot find the file specified",
        ],
        none: &["could not load file or assembly"],
        summary: "A required command-line tool could not be started.",
        cause: "DevHub inherits the PATH captured when you signed in to Windows, so a tool installed since then can be invisible to the app even though it works in a terminal.",
        steps: &[
            "Open Settings and use Re-scan under Command-line tools.",
            "If the tool is still not found, use Locate… to point DevHub at its executable.",
            "If it is genuinely missing, install it: clio via `dotnet tool install clio -g`, GitHub CLI via `winget install GitHub.cli`.",
        ],
    },
    Rule {
        code: "git-push-rejected",
        all: &[],
        any: &["non-fast-forward", "updates were rejected", "fetch first"],
        none: &[],
        summary: "The push was rejected because the remote has commits you do not have.",
        cause: "Someone else pushed to this branch after your last pull, so your history is behind the remote. Overwriting it would drop their work.",
        steps: &[
            "Pull the remote changes into the workspace first.",
            "Resolve any conflicts locally.",
            "Push again once your branch includes the remote commits.",
        ],
    },
    Rule {
        code: "git-repo-access",
        all: &["repository not found"],
        any: &[],
        none: &[],
        summary: "GitHub cannot see that repository under the signed-in account.",
        cause: "GitHub returns \"not found\" rather than \"forbidden\" for private repositories you lack access to, so this is usually the wrong account rather than a missing repository.",
        steps: &[
            "Check which account is active under Settings → GitHub and Git identity.",
            "Switch to an account that can access the repository, or have its owner add you as a collaborator.",
            "Accept the collaborator invitation, then retry.",
        ],
    },
    Rule {
        code: "git-auth",
        all: &[],
        any: &[
            "could not read username",
            "terminal prompts disabled",
            "authentication failed for",
        ],
        none: &[],
        summary: "Git could not authenticate to the remote.",
        cause: "DevHub disables interactive Git prompts so jobs cannot hang, so a missing or expired credential fails immediately instead of asking.",
        steps: &[
            "Sign in again under Settings → GitHub and Git identity.",
            "Confirm the remote URL uses HTTPS rather than SSH.",
            "Retry the push or pull.",
        ],
    },
    Rule {
        code: "creatio-auth",
        all: &[],
        any: &[
            "unauthorized",
            "invalid username or password",
            "the user name or password is incorrect",
            "401",
        ],
        none: &["repository not found", "github.com"],
        summary: "Creatio rejected the environment's credentials.",
        cause: "The stored user name or password for this environment is wrong or expired, or the account is locked in Creatio.",
        steps: &[
            "Open the Environments screen and re-enter the credentials for this environment.",
            "Confirm the account can sign in to Creatio directly in a browser.",
            "Check that the account is not locked or password-expired in Creatio.",
        ],
    },
    Rule {
        code: "creatio-unreachable",
        all: &[],
        any: &[
            "no such host",
            "connection refused",
            "actively refused",
            "timed out",
            "unable to connect",
            "name or service not known",
            "503",
            "502",
        ],
        none: &[],
        summary: "The Creatio environment could not be reached.",
        cause: "The URL, the site, or the network path to it is unavailable — a stopped local IIS site and a VPN that is not connected both look like this.",
        steps: &[
            "Open the environment URL in a browser to confirm it responds.",
            "For a local environment, check the site is started in IIS.",
            "For a remote environment, confirm any required VPN is connected.",
            "Verify the URL under the Environments screen.",
        ],
    },
    Rule {
        code: "creatio-forbidden",
        all: &[],
        any: &["forbidden", "403"],
        none: &["repository not found", "github.com"],
        summary: "Creatio accepted the sign-in but refused the operation.",
        cause: "The environment account authenticated correctly but lacks the rights the operation needs — package management and SQL execution both require elevated access.",
        steps: &[
            "Use an environment account with system-administrator rights.",
            "Confirm the operation is permitted in this environment — some hosted environments block direct SQL.",
        ],
    },
];

fn matches(rule: &Rule, haystack: &str) -> bool {
    rule.all.iter().all(|needle| haystack.contains(needle))
        && (rule.any.is_empty() || rule.any.iter().any(|needle| haystack.contains(needle)))
        && !rule.none.iter().any(|needle| haystack.contains(needle))
}

/// The best-known explanation for `raw`, if DevHub recognizes it.
pub fn diagnose(raw: &str) -> Option<Diagnosis> {
    let haystack = raw.to_lowercase();
    RULES.iter().find(|rule| matches(rule, &haystack)).map(|rule| Diagnosis {
        code: rule.code.to_string(),
        summary: rule.summary.to_string(),
        cause: rule.cause.to_string(),
        steps: rule.steps.iter().map(|step| step.to_string()).collect(),
    })
}

/// Diagnose the end of a job log, where the failure almost always is. Scanning
/// only the tail keeps an early warning line from outranking the real error.
pub fn diagnose_log(lines: &[String]) -> Option<Diagnosis> {
    const TAIL: usize = 40;
    let start = lines.len().saturating_sub(TAIL);
    diagnose(&lines[start..].join("\n"))
}

/// Explain an error string for the frontend's inline error areas.
#[tauri::command]
pub fn diagnose_error(text: String) -> Option<Diagnosis> {
    diagnose(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_for(raw: &str) -> Option<String> {
        diagnose(raw).map(|d| d.code)
    }

    #[test]
    fn recognizes_a_damaged_clio_install() {
        // Real output from `clio list-apps` on a partial update.
        let raw = "[ERR] - Could not load file or assembly 'Creatio.Metrics.Abstractions, Version=1.0.5.0'.";
        assert_eq!(code_for(raw).as_deref(), Some("clio-damaged"));
    }

    #[test]
    fn recognizes_a_locked_clio_tool_store() {
        let raw = "Failed to uninstall tool package 'clio': Access to the path 'C:\\Users\\x\\.dotnet\\tools\\.store' is denied.";
        assert_eq!(code_for(raw).as_deref(), Some("clio-locked"));
    }

    #[test]
    fn a_damaged_install_outranks_the_generic_missing_file_rule() {
        // "The system cannot find the file specified" also appears in the
        // tool-missing rule; the more specific assembly failure must win.
        let raw = "Could not load file or assembly 'X'. The system cannot find the file specified.";
        assert_eq!(code_for(raw).as_deref(), Some("clio-damaged"));
    }

    #[test]
    fn recognizes_a_missing_executable() {
        let raw = "'gh' is not recognized as an internal or external command, operable program or batch file.";
        let found = diagnose(raw).expect("tool-missing");
        assert_eq!(found.code, "tool-missing");
        assert!(found.steps.iter().any(|step| step.contains("Command-line tools")));
    }

    #[test]
    fn separates_github_access_from_creatio_auth() {
        let git = "remote: Repository not found.\nfatal: repository 'https://github.com/x/y.git/' not found";
        assert_eq!(code_for(git).as_deref(), Some("git-repo-access"));

        let creatio = "[ERR] - Unauthorized (401) while calling the Creatio dataservice";
        assert_eq!(code_for(creatio).as_deref(), Some("creatio-auth"));
    }

    #[test]
    fn recognizes_a_rejected_push() {
        let raw = "! [rejected] main -> main (non-fast-forward)\nUpdates were rejected because the tip of your current branch is behind";
        assert_eq!(code_for(raw).as_deref(), Some("git-push-rejected"));
    }

    #[test]
    fn recognizes_an_unreachable_environment() {
        let raw = "No connection could be made because the target machine actively refused it 127.0.0.1:8080";
        assert_eq!(code_for(raw).as_deref(), Some("creatio-unreachable"));
    }

    #[test]
    fn unknown_failures_fall_through_to_raw_output() {
        assert_eq!(diagnose("some unexpected failure"), None);
        assert_eq!(diagnose(""), None);
    }

    #[test]
    fn log_diagnosis_reads_the_tail_not_the_header() {
        let mut lines: Vec<String> = vec!["[INF] - starting".to_string(); 60];
        lines.push("[ERR] - Could not load file or assembly 'X'".to_string());
        assert_eq!(diagnose_log(&lines).map(|d| d.code).as_deref(), Some("clio-damaged"));

        // An error scrolled far above the tail is deliberately not reported.
        let mut buried = vec!["[ERR] - Could not load file or assembly 'X'".to_string()];
        buried.extend(vec!["[INF] - working".to_string(); 100]);
        assert_eq!(diagnose_log(&buried), None);
    }

    #[test]
    fn every_rule_has_a_cause_and_at_least_one_step() {
        for rule in RULES {
            assert!(!rule.cause.is_empty(), "{} has no cause", rule.code);
            assert!(!rule.steps.is_empty(), "{} has no steps", rule.code);
            assert!(
                !rule.all.is_empty() || !rule.any.is_empty(),
                "{} would match everything",
                rule.code
            );
        }
    }

    #[test]
    fn rule_needles_are_lowercase_so_they_can_match() {
        // diagnose() lowercases the haystack; an uppercase needle could never hit.
        for rule in RULES {
            for needle in rule.all.iter().chain(rule.any).chain(rule.none) {
                assert_eq!(*needle, needle.to_lowercase(), "{} has a non-lowercase needle", rule.code);
            }
        }
    }
}
