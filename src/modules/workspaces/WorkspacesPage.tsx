import { useEffect, useState } from "react";
import { listWorkspaces, onWorkspacesChanged, removeWorkspace, WorkspaceSummary } from "../../lib/ipc";
import DeployFromGithubDialog from "./DeployFromGithubDialog";
import NewWorkspaceWizard from "./NewWorkspaceWizard";
import WorkspaceDetail from "./WorkspaceDetail";

export default function WorkspacesPage({
  onShowJobs,
  initialWorkspaceId,
  onWorkspaceClosed,
}: {
  onShowJobs: () => void;
  initialWorkspaceId?: string | null;
  onWorkspaceClosed?: () => void;
}) {
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(initialWorkspaceId ?? null);
  const [showWizard, setShowWizard] = useState(false);
  const [showDeploy, setShowDeploy] = useState(false);

  const refresh = () => listWorkspaces().then(setWorkspaces).catch(console.error);

  useEffect(() => {
    refresh();
    const un = onWorkspacesChanged(refresh);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const selected = workspaces.find((w) => w.id === selectedId);
  if (selected) {
    return (
      <WorkspaceDetail
        workspace={selected}
        onBack={() => {
          setSelectedId(null);
          onWorkspaceClosed?.();
        }}
        onChanged={refresh}
        onShowJobs={onShowJobs}
      />
    );
  }

  const fmtWhen = (ts: number | null) => (ts ? new Date(ts).toLocaleString() : "never");

  return (
    <div className="page-body">
      <div className="page-bar">
        <h1>Workspaces</h1>
        <div className="ws-actions">
          <button className="ghost" onClick={() => setShowDeploy(true)}>
            ⬇ Deploy from GitHub
          </button>
          <button className="primary" onClick={() => setShowWizard(true)}>
            + New workspace
          </button>
        </div>
      </div>
      {workspaces.length === 0 && (
        <p className="empty">
          No workspaces yet. A workspace is a local git folder holding package source code pulled from an environment.
        </p>
      )}
      <div className="env-grid">
        {workspaces.map((w) => (
          <div className="env-card" key={w.id}>
            <div className="env-head">
              <strong>{w.name}</strong>
              {!w.exists && <span className="pill bad">folder missing</span>}
              {w.dirtyCount > 0 ? (
                <span className="pill warn">{w.dirtyCount} uncommitted</span>
              ) : (
                w.exists && <span className="pill ok">clean</span>
              )}
            </div>
            <div className="env-uri" title={w.path}>
              {w.path}
            </div>
            <div className="env-meta">
              <span className="pill dim">{w.env}</span>
              {w.branch && <span className="pill accent">{w.branch}</span>}
              {w.remote ? <span className="pill ok">remote ✓</span> : <span className="pill dim">no remote</span>}
            </div>
            <div className="env-uri">
              last pull: {fmtWhen(w.lastPull)} · last push: {fmtWhen(w.lastPush)}
            </div>
            <div className="env-actions">
              <button onClick={() => setSelectedId(w.id)} disabled={!w.exists}>
                Open
              </button>
              <button
                onClick={() => {
                  if (confirm(`Remove "${w.name}" from the list? The folder on disk is kept.`)) {
                    removeWorkspace(w.id).then(refresh);
                  }
                }}
              >
                Remove
              </button>
            </div>
          </div>
        ))}
      </div>
      {showWizard && (
        <NewWorkspaceWizard
          onClose={() => setShowWizard(false)}
          onStarted={() => {
            setShowWizard(false);
            refresh();
          }}
        />
      )}
      {showDeploy && (
        <DeployFromGithubDialog
          onClose={() => setShowDeploy(false)}
          onStarted={() => {
            setShowDeploy(false);
            onShowJobs();
          }}
        />
      )}
    </div>
  );
}
