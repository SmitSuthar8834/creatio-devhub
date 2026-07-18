import { useCallback, useEffect, useState } from "react";
import {
  Commit,
  FileChange,
  onJobUpdate,
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
