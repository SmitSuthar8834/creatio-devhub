import { useEffect, useState } from "react";
import { Download, Plus } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
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
    <div className="mx-auto grid max-w-5xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Workspaces</h1>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => setShowDeploy(true)}>
            <Download aria-hidden="true" />
            Deploy from GitHub
          </Button>
          <Button onClick={() => setShowWizard(true)}>
            <Plus aria-hidden="true" />
            New workspace
          </Button>
        </div>
      </div>

      {workspaces.length === 0 && (
        <p className="text-muted-foreground">
          No workspaces yet. A workspace is a local git folder holding package source code pulled
          from an environment.
        </p>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {workspaces.map((w) => (
          <Card key={w.id}>
            <CardHeader>
              <CardTitle className="flex flex-wrap items-center gap-2 text-base">
                {w.name}
                {!w.exists && <Badge variant="destructive">folder missing</Badge>}
                {w.dirtyCount > 0 ? (
                  <Badge className="border-transparent bg-warning/15 text-warning">
                    {w.dirtyCount} uncommitted
                  </Badge>
                ) : (
                  w.exists && (
                    <Badge className="border-transparent bg-success/15 text-success">clean</Badge>
                  )
                )}
              </CardTitle>
              <p className="truncate font-mono text-xs text-muted-foreground" title={w.path}>
                {w.path}
              </p>
            </CardHeader>
            <CardContent className="grid gap-2">
              <div className="flex flex-wrap gap-1.5">
                <Badge variant="secondary">{w.env}</Badge>
                {w.branch && (
                  <Badge className="border-transparent bg-accent/15 text-accent-foreground">
                    {w.branch}
                  </Badge>
                )}
                {w.remote ? (
                  <Badge className="border-transparent bg-success/15 text-success">remote ✓</Badge>
                ) : (
                  <Badge variant="outline" className="text-muted-foreground">no remote</Badge>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                last pull: {fmtWhen(w.lastPull)} · last push: {fmtWhen(w.lastPush)}
              </p>
            </CardContent>
            <CardFooter className="gap-2">
              <Button size="sm" variant="outline" onClick={() => setSelectedId(w.id)} disabled={!w.exists}>
                Open
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="text-destructive hover:text-destructive"
                onClick={() => {
                  if (confirm(`Remove "${w.name}" from the list? The folder on disk is kept.`)) {
                    removeWorkspace(w.id).then(refresh);
                  }
                }}
              >
                Remove
              </Button>
            </CardFooter>
          </Card>
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
