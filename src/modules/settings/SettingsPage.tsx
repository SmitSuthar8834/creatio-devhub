import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, Update } from "@tauri-apps/plugin-updater";
import {
  EnvSummary, getGithubStatus, getToolPaths, GithubStatus, listEnvironments, onJobUpdate,
  setDefaultEnvironment, setGitIdentity, setToolPath, startGithubLogin, ToolPath,
} from "../../lib/ipc";

export default function SettingsPage() {
  const [environments, setEnvironments] = useState<EnvSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [saved, setSaved] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [github, setGithub] = useState<GithubStatus | null>(null);
  const [gitName, setGitName] = useState("");
  const [gitEmail, setGitEmail] = useState("");
  const [githubJob, setGithubJob] = useState<string | null>(null);
  const [githubNotice, setGithubNotice] = useState("");
  const [githubError, setGithubError] = useState("");
  const [tools, setTools] = useState<ToolPath[]>([]);
  const [toolError, setToolError] = useState("");
  const [appVersion, setAppVersion] = useState("");
  const [update, setUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState("Ready to check for updates.");
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);

  const load = async () => {
    try {
      const list = await listEnvironments();
      setEnvironments(list);
      const active = list.find((environment) => environment.isActive) ?? list[0];
      setSelected(active?.name ?? "");
      setSaved(active?.name ?? "");
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    load();
    refreshGithub();
    refreshTools();
    getVersion().then(setAppVersion);
  }, []);

  const checkForUpdate = async () => {
    setUpdateBusy(true);
    setUpdateProgress(null);
    setUpdateStatus("Checking GitHub Releases…");
    try {
      const available = await check({ timeout: 15000 });
      setUpdate(available);
      setUpdateStatus(available
        ? `Version ${available.version} is available.`
        : "You already have the latest version.");
    } catch (reason) {
      setUpdate(null);
      setUpdateStatus(`Update check failed: ${String(reason)}`);
    } finally {
      setUpdateBusy(false);
    }
  };

  const installUpdate = async () => {
    if (!update) return;
    setUpdateBusy(true);
    setUpdateStatus(`Downloading version ${update.version}…`);
    let downloaded = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          setUpdateProgress(0);
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setUpdateProgress(total ? Math.min(100, Math.round(downloaded / total * 100)) : null);
        } else if (event.event === "Finished") {
          setUpdateProgress(100);
          setUpdateStatus("Update installed. Restarting DevHub…");
        }
      });
      await relaunch();
    } catch (reason) {
      setUpdateStatus(`Update installation failed: ${String(reason)}`);
      setUpdateBusy(false);
    }
  };

  useEffect(() => {
    const unlisten = onJobUpdate((job) => {
      if (job.id === githubJob && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        setGithubJob(null);
        if (job.status === "succeeded") {
          setGithubNotice("GitHub sign-in completed.");
          refreshGithub();
        } else if (job.status === "cancelled") {
          setGithubNotice("GitHub sign-in was cancelled.");
        } else {
          setGithubError("GitHub sign-in failed. Open Jobs to review the output.");
        }
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [githubJob]);

  const refreshGithub = async () => {
    try {
      const status = await getGithubStatus();
      setGithub(status);
      setGitName(status.gitName ?? status.accountName ?? status.login ?? "");
      setGitEmail(status.gitEmail ?? status.suggestedEmail ?? "");
    } catch (e) {
      setGithubError(String(e));
    }
  };

  const refreshTools = async () => {
    setToolError("");
    try {
      setTools(await getToolPaths());
    } catch (e) {
      setToolError(String(e));
    }
  };

  /// Pin a CLI to an explicit executable, for installs in a location DevHub
  /// does not know about. Cancelling the picker leaves the setting alone.
  const pickTool = async (program: string) => {
    const picked = await open({
      title: `Select the ${program} executable`,
      multiple: false,
      directory: false,
    });
    if (typeof picked !== "string") return;
    await applyToolPath(program, picked);
  };

  const applyToolPath = async (program: string, path: string) => {
    setToolError("");
    try {
      await setToolPath(program, path);
      await refreshTools();
      await refreshGithub();
    } catch (e) {
      setToolError(String(e));
    }
  };

  const save = async () => {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await setDefaultEnvironment(selected);
      setSaved(selected);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const loginGithub = async () => {
    setGithubError("");
    setGithubNotice("");
    try {
      setGithubJob(await startGithubLogin());
      setGithubNotice("GitHub opened a browser sign-in flow. Complete it there; progress is also shown in Jobs.");
    } catch (e) {
      setGithubError(String(e));
    }
  };

  const saveIdentity = async () => {
    setGithubError("");
    setGithubNotice("");
    try {
      await setGitIdentity(gitName, gitEmail);
      setGithubNotice("Git commit identity saved globally.");
      await refreshGithub();
    } catch (e) {
      setGithubError(String(e));
    }
  };

  return (
    <div className="page-body">
      <div className="page-bar">
        <div><h1>Settings</h1><p>Manage your environments, identity, and DevHub installation.</p></div>
      </div>

      <section className="settings-card update-card">
        <div className="settings-card-heading">
          <div className="settings-symbol">↥</div>
          <div><h2>DevHub updates</h2><p className="hint">Securely download signed releases published from GitHub.</p></div>
          <span className="version-badge">v{appVersion || "…"}</span>
        </div>
        <p className={updateStatus.startsWith("Update check failed") || updateStatus.startsWith("Update installation failed") ? "form-error" : "update-status"}>
          {updateStatus}
        </p>
        {updateProgress !== null && <div className="update-progress"><span style={{ width: `${updateProgress}%` }} /></div>}
        {update?.body && <p className="update-notes">{update.body}</p>}
        <div className="settings-actions">
          <button className="ghost" onClick={checkForUpdate} disabled={updateBusy}>
            {updateBusy && !update ? "Checking…" : "Check for updates"}
          </button>
          {update && <button className="primary" onClick={installUpdate} disabled={updateBusy}>
            {updateBusy ? "Installing…" : `Install v${update.version} and restart`}
          </button>}
        </div>
      </section>

      <section className="settings-card">
        <h2>Default environment</h2>
        <p className="hint">
          This changes clio&apos;s active environment. DevHub uses it as the initial selection
          when creating workspaces, browsing packages, and starting environment operations.
        </p>
        {environments.length === 0 ? (
          <p className="empty">Register an environment before choosing a default.</p>
        ) : (
          <div className="settings-row">
            <label>
              Environment
              <select value={selected} onChange={(event) => setSelected(event.target.value)}>
                {environments.map((environment) => (
                  <option key={environment.name} value={environment.name}>
                    {environment.name} — {environment.uri}
                  </option>
                ))}
              </select>
            </label>
            <button className="primary" disabled={busy || !selected || selected === saved} onClick={save}>
              {busy ? "Saving…" : "Save default"}
            </button>
          </div>
        )}
        {saved && <p className="notice">Current default: <strong>{saved}</strong></p>}
        {error && <p className="form-error">{error}</p>}
      </section>

      <section className="settings-card">
        <h2>GitHub and Git identity</h2>
        <p className="hint">
          GitHub authentication controls which account pushes over HTTPS. Git name and email
          control the author recorded in new commits.
        </p>
        {!github?.ghInstalled ? (
          <div className="tool-missing">
            <p className="form-error">DevHub could not start the GitHub CLI (gh).</p>
            <p className="hint">
              If gh is installed, it was most likely added to PATH after you last signed in to
              Windows — DevHub inherits the sign-in PATH. Use Refresh status, or point DevHub at
              gh.exe directly under Command-line tools below. Otherwise install it from{" "}
              <code>winget install GitHub.cli</code>.
            </p>
            {github?.ghError && <p className="hint mono">{github.ghError}</p>}
          </div>
        ) : github.authenticated ? (
          <p className="notice">
            Signed in to GitHub as <strong>{github.login}</strong>
            {github.accountName ? ` (${github.accountName})` : ""}.
          </p>
        ) : (
          <p className="form-error">GitHub is not signed in.</p>
        )}
        <div className="settings-actions">
          <button className="ghost" onClick={loginGithub} disabled={!github?.ghInstalled || !!githubJob}>
            {githubJob ? "Waiting for sign-in…" : github?.authenticated ? "Switch GitHub account" : "Sign in to GitHub"}
          </button>
          <button className="ghost" onClick={() => { refreshGithub(); refreshTools(); }}>Refresh status</button>
        </div>
        <div className="settings-identity">
          <label>
            Git author name
            <input value={gitName} onChange={(event) => setGitName(event.target.value)} />
          </label>
          <label>
            Git author email
            <input value={gitEmail} onChange={(event) => setGitEmail(event.target.value)} />
          </label>
          <button className="primary" onClick={saveIdentity}>Save Git identity</button>
        </div>
        {githubNotice && <p className="notice">{githubNotice}</p>}
        {githubError && <p className="form-error">{githubError}</p>}
      </section>

      <section className="settings-card">
        <h2>Command-line tools</h2>
        <p className="hint">
          DevHub drives these CLIs directly. It searches PATH — including the current system PATH,
          not just the one inherited at sign-in — and the usual install locations. Pin a path here
          if a tool lives somewhere else.
        </p>
        <table className="tool-table">
          <tbody>
            {tools.map((tool) => (
              <tr key={tool.program}>
                <td className="tool-name">{tool.program}</td>
                <td className="tool-path">
                  {tool.path
                    ? <span className="mono">{tool.path}</span>
                    : <span className="form-error">Not found. Searched: {tool.searched.join(", ")}</span>}
                  {tool.custom && <span className="hint"> (pinned)</span>}
                </td>
                <td className="tool-actions">
                  <button className="ghost" onClick={() => pickTool(tool.program)}>Locate…</button>
                  {tool.custom && (
                    <button className="ghost" onClick={() => applyToolPath(tool.program, "")}>Reset</button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="settings-actions">
          <button className="ghost" onClick={refreshTools}>Re-scan</button>
        </div>
        {toolError && <p className="form-error">{toolError}</p>}
      </section>
    </div>
  );
}
