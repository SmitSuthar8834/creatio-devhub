import { useCallback, useEffect, useState } from "react";
import {
  addPackageToWorkspace,
  Commit,
  createGithubRepo,
  FileChange,
  listPackages,
  onJobUpdate,
  PackageInfo,
  pullWorkspace,
  RemoteStatus,
  pushWorkspaceCloud,
  wsCommit,
  wsDiff,
  wsLog,
  wsPushRemote,
  wsRemoteStatus,
  wsSetRemote,
  wsStatus,
  WorkspaceSummary,
} from "../../lib/ipc";

interface Props {
  workspace: WorkspaceSummary;
  onBack: () => void;
  onChanged: () => void;
  onShowJobs: () => void;
}

export default function WorkspaceDetail({ workspace: w, onBack, onChanged, onShowJobs }: Props) {
  const [tab, setTab] = useState<"changes" | "history">("changes");
  const [changes, setChanges] = useState<FileChange[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [diff, setDiff] = useState("");
  const [commits, setCommits] = useState<Commit[]>([]);
  const [message, setMessage] = useState("");
  const [remoteInput, setRemoteInput] = useState(w.remote ?? "");
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [showPush, setShowPush] = useState(false);
  const [skipBackup, setSkipBackup] = useState(false);
  const [drift, setDrift] = useState("");
  const [remoteStatus, setRemoteStatus] = useState<RemoteStatus | null>(null);
  const [remoteError, setRemoteError] = useState("");
  const [showAddPkg, setShowAddPkg] = useState(false);
  const [pkgList, setPkgList] = useState<PackageInfo[]>([]);
  const [pkgFilter, setPkgFilter] = useState("");
  const [pkgLoading, setPkgLoading] = useState(false);
  const [showCreateRepo, setShowCreateRepo] = useState(false);
  const [repoName, setRepoName] = useState(w.name);
  const [repoPrivate, setRepoPrivate] = useState(true);

  const refresh = useCallback(() => {
    wsStatus(w.id).then(setChanges).catch((e) => setError(String(e)));
    wsLog(w.id).then(setCommits).catch(() => setCommits([]));
  }, [w.id]);

  const checkRemote = useCallback(() => {
    if (!w.remote) return;
    setRemoteError("");
    wsRemoteStatus(w.id).then((status) => {
      setRemoteStatus(status);
      setRemoteError("");
      setError((previous) =>
        previous.includes("git fetch") || previous.includes("Repository not found")
          ? ""
          : previous,
      );
    }).catch((e) => setRemoteError(String(e)));
  }, [w.id, w.remote]);

  useEffect(() => {
    refresh();
    checkRemote();
    const timer = window.setInterval(checkRemote, 60_000);
    const un = onJobUpdate((job) => {
      if (job.env === w.env && (job.status === "succeeded" || job.status === "failed")) refresh();
    });
    return () => {
      un.then((f) => f());
      window.clearInterval(timer);
    };
  }, [refresh, checkRemote, w.env]);

  const showDiff = (file: string) => {
    setSelectedFile(file);
    setDiff("loading…");
    wsDiff(w.id, file).then(setDiff).catch((e) => setDiff(String(e)));
  };

  const doPull = async () => {
    setError("");
    setNotice("");
    try {
      await pullWorkspace(w.id);
      setNotice("Pull started — follow it on the Jobs screen.");
    } catch (e) {
      setError(String(e));
    }
  };

  const doCommit = async () => {
    setError("");
    try {
      const summary = await wsCommit(w.id, message);
      setNotice(`Committed: ${summary}`);
      setMessage("");
      setSelectedFile(null);
      setDiff("");
      refresh();
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const doPushCloud = async (force: boolean) => {
    setError("");
    setNotice("");
    setDrift("");
    setShowPush(false);
    try {
      await pushWorkspaceCloud(w.id, force, skipBackup);
      setNotice("Push to Cloud started — the server-side compile takes minutes. Follow it on the Jobs screen; you'll get a notification when it finishes.");
    } catch (e) {
      const msg = String(e);
      if (msg.includes("DRIFT:")) {
        setDrift(msg.replace(/^.*DRIFT:\s*/, ""));
      } else {
        setError(msg);
      }
    }
  };

  const doPush = async () => {
    setError("");
    setNotice("");
    try {
      if (remoteInput.trim() && remoteInput.trim() !== (w.remote ?? "")) {
        await wsSetRemote(w.id, remoteInput.trim());
      }
      await wsPushRemote(w.id);
      setNotice("Push started — follow it on the Jobs screen.");
      onChanged();
    } catch (e) {
      const message = String(e);
      if (message.includes("REMOTE_AHEAD:")) {
        setError(message.replace(/^.*REMOTE_AHEAD:\s*/, ""));
        checkRemote();
      } else {
        setError(message);
      }
    }
  };

  const openAddPkg = () => {
    setShowAddPkg(true);
    setPkgFilter("");
    setPkgLoading(true);
    listPackages(w.env)
      .then((res) => setPkgList(res.items))
      .catch((e) => setError(String(e)))
      .finally(() => setPkgLoading(false));
  };

  const doAddPackage = async (name: string) => {
    setError("");
    setNotice("");
    setShowAddPkg(false);
    try {
      await addPackageToWorkspace(w.id, name);
      setNotice(`Adding ${name} to the workspace — follow it on the Jobs screen.`);
    } catch (e) {
      setError(String(e));
    }
  };

  const doCreateRepo = async () => {
    setError("");
    setNotice("");
    setShowCreateRepo(false);
    try {
      await createGithubRepo({ id: w.id, repoName: repoName.trim(), private: repoPrivate, push: true });
      setNotice("Creating the GitHub repository and pushing — follow it on the Jobs screen.");
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const filteredPkgs = pkgList.filter((p) =>
    p.name.toLowerCase().includes(pkgFilter.trim().toLowerCase()),
  );

  const hasPackages = changes.length > 0 || (w.lastPull != null);
  const hasRepo = !!w.remote;
  const isPushed = !!remoteStatus?.hasRemote && remoteStatus.ahead === 0;
  const showGuidance = !hasPackages || !hasRepo;

  const suggested = `Pull from ${w.env} — ${changes.length} file(s) changed`;

  return (
    <div className="page-body">
      <div className="page-bar">
        <div className="ws-title">
          <button className="ghost back" onClick={onBack}>
            ←
          </button>
          <h1>{w.name}</h1>
          <span className="pill dim">{w.env}</span>
          {w.branch && <span className="pill accent">{w.branch}</span>}
        </div>
        <div className="ws-actions">
          <button className="ghost" onClick={openAddPkg}>
            ➕ Add package
          </button>
          <button className="ghost" onClick={doPull}>
            ⬇ Pull from Cloud
          </button>
          <button className="primary" onClick={() => setShowPush(true)}>
            ⬆ Push to Cloud
          </button>
          <button className="ghost" onClick={onShowJobs}>
            Jobs
          </button>
        </div>
      </div>

      {notice && <p className="notice">{notice}</p>}
      {error && <p className="form-error">{error}</p>}

      {showGuidance && (
        <div className="guidance-banner">
          <div className="guidance-head">
            <strong>{hasPackages ? "Almost there." : "Your workspace is ready — but empty."}</strong>
            <span>
              {hasPackages
                ? "Publish it to GitHub so your work is versioned and shareable."
                : "Add the Creatio packages you want to version-control — only the ones you pick get downloaded."}
            </span>
          </div>
          <div className="guidance-steps">
            <span className="gstep done">✅ Workspace</span>
            <span className="garrow">→</span>
            <span className={`gstep ${hasPackages ? "done" : "next"}`}>
              {hasPackages ? "✅" : "⬜"} Packages
            </span>
            <span className="garrow">→</span>
            <span className={`gstep ${hasRepo ? "done" : hasPackages ? "next" : ""}`}>
              {hasRepo ? "✅" : "⬜"} GitHub repo
            </span>
            <span className="garrow">→</span>
            <span className={`gstep ${isPushed ? "done" : ""}`}>{isPushed ? "✅" : "⬜"} Pushed</span>
          </div>
          <div className="dialog-actions">
            {!hasPackages && (
              <button className="primary" onClick={openAddPkg}>
                ➕ Add packages
              </button>
            )}
            {hasPackages && !hasRepo && (
              <button className="primary" onClick={() => setShowCreateRepo(true)}>
                Create GitHub repo
              </button>
            )}
          </div>
        </div>
      )}

      {showAddPkg && (
        <div className="dialog-backdrop" onClick={() => setShowAddPkg(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <h2>Add package from {w.env}</h2>
            <p className="hint">Pick a package to include in this workspace. It gets added to the selection and its source is pulled in as a Git change.</p>
            <input
              autoFocus
              placeholder="Filter packages…"
              value={pkgFilter}
              onChange={(e) => setPkgFilter(e.target.value)}
            />
            <div className="pkg-picker">
              {pkgLoading ? (
                <p className="empty">Loading packages…</p>
              ) : filteredPkgs.length === 0 ? (
                <p className="empty">No packages match.</p>
              ) : (
                filteredPkgs.map((p) => (
                  <button key={p.name} className="pkg-row" onClick={() => doAddPackage(p.name)}>
                    <span className="pkg-name">{p.name}</span>
                    <span className="pkg-meta">{p.maintainer} · {p.version}</span>
                  </button>
                ))
              )}
            </div>
            <div className="dialog-actions">
              <button className="ghost" onClick={() => setShowAddPkg(false)}>Close</button>
            </div>
          </div>
        </div>
      )}

      {showCreateRepo && (
        <div className="dialog-backdrop" onClick={() => setShowCreateRepo(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <h2>Create GitHub repository</h2>
            <p className="hint">Creates the repository on your signed-in GitHub account, wires it as <code>origin</code>, and pushes the current commit. Requires the GitHub CLI signed in (Settings → GitHub).</p>
            <label>
              Repository name
              <input value={repoName} onChange={(e) => setRepoName(e.target.value)} placeholder="my-workspace" autoFocus />
            </label>
            <label className="check-row">
              <input type="checkbox" checked={repoPrivate} onChange={(e) => setRepoPrivate(e.target.checked)} />
              Private repository (recommended)
            </label>
            <div className="dialog-actions">
              <button className="ghost" onClick={() => setShowCreateRepo(false)}>Cancel</button>
              <button className="primary" onClick={doCreateRepo} disabled={!repoName.trim()}>
                Create &amp; push
              </button>
            </div>
          </div>
        </div>
      )}

      {drift && (
        <div className="drift-banner">
          <p>⚠ {drift}</p>
          <div className="dialog-actions">
            <button className="ghost" onClick={() => setDrift("")}>
              Cancel
            </button>
            <button
              className="ghost"
              onClick={() => {
                setDrift("");
                doPull();
              }}
            >
              Pull first
            </button>
            <button className="primary" onClick={() => doPushCloud(true)}>
              Push anyway
            </button>
          </div>
        </div>
      )}
      {showPush && (
        <div className="dialog-backdrop" onClick={() => setShowPush(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <h2>Push to {w.env}?</h2>
            <p className="hint">
              Packs this workspace and installs it into the environment. The server compiles the configuration — expect
              several minutes. The job can't be safely cancelled once installation starts.
            </p>
            <label className="check-row">
              <input type="checkbox" checked={!skipBackup} onChange={(e) => setSkipBackup(!e.target.checked)} />
              Create a backup on the environment first (recommended)
            </label>
            {changes.length > 0 && (
              <p className="form-error">Note: {changes.length} uncommitted change(s) will be pushed as-is. Consider committing first.</p>
            )}
            <div className="dialog-actions">
              <button className="ghost" onClick={() => setShowPush(false)}>
                Cancel
              </button>
              <button className="primary" onClick={() => doPushCloud(false)}>
                Push to Cloud
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="w-tabs">
        <button className={tab === "changes" ? "on" : ""} onClick={() => setTab("changes")}>
          Changes ({changes.length})
        </button>
        <button className={tab === "history" ? "on" : ""} onClick={() => setTab("history")}>
          History
        </button>
      </div>

      {tab === "changes" && (
        <>
          {changes.length === 0 ? (
            <p className="empty">Working tree is clean. Pull from Cloud to fetch the latest package changes.</p>
          ) : (
            <div className="changes-split">
              <div className="filelist">
                {changes.map((c) => (
                  <button
                    key={c.path}
                    className={`file-row ${selectedFile === c.path ? "sel" : ""}`}
                    onClick={() => showDiff(c.path)}
                  >
                    <span className={`fstatus s-${c.status}`}>{c.status}</span>
                    <span className="fpath" title={c.path}>
                      {c.path}
                    </span>
                  </button>
                ))}
              </div>
              <pre className="diffview">
                {selectedFile
                  ? diff.split("\n").map((l, i) => (
                      <span
                        key={i}
                        className={l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : l.startsWith("@@") ? "hunk" : ""}
                      >
                        {l + "\n"}
                      </span>
                    ))
                  : "Select a file to see its diff."}
              </pre>
            </div>
          )}
          {changes.length > 0 && (
            <div className="commit-bar">
              <input
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder={suggested}
                onFocus={() => !message && setMessage(suggested)}
              />
              <button className="primary" onClick={doCommit}>
                Commit
              </button>
            </div>
          )}
        </>
      )}

      {tab === "history" && (
        <>
          {remoteStatus && remoteStatus.behind > 0 && (
            <div className="drift-banner">
              <p>
                ⚠ Another contributor pushed {remoteStatus.behind} commit(s) to origin/{remoteStatus.branch}.
                Your push is blocked until you pull or rebase those changes, which may conflict with local work.
              </p>
              <button className="ghost" onClick={checkRemote}>Check again</button>
            </div>
          )}
          {remoteStatus && remoteStatus.behind === 0 && remoteStatus.hasRemote && (
            <p className="notice">
              Remote is current{remoteStatus.ahead > 0 ? ` · ${remoteStatus.ahead} local commit(s) ready to push` : ""}.
            </p>
          )}
          {remoteError && <div className="drift-banner">
            <p>Remote check failed: {remoteError}</p>
            <button className="ghost" onClick={checkRemote}>Check again</button>
          </div>}
          <div className="remote-bar">
            <input
              value={remoteInput}
              onChange={(e) => setRemoteInput(e.target.value)}
              placeholder="git remote URL (https://github.com/you/repo.git)"
            />
            <button className="ghost" onClick={doPush}>
              ⬆ Push to remote
            </button>
            {!hasRepo && (
              <button className="ghost" onClick={() => setShowCreateRepo(true)}>
                ✨ Create GitHub repo
              </button>
            )}
          </div>
          {commits.length === 0 ? (
            <p className="empty">No commits yet.</p>
          ) : (
            <div className="commits">
              {commits.map((c) => (
                <div className="commit-row" key={c.hash}>
                  <code>{c.hash}</code>
                  <span className="cmsg">{c.message}</span>
                  <span className="cmeta">
                    {c.author} · {c.date}
                  </span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
