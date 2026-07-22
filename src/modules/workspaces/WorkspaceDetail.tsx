import { useCallback, useEffect, useState } from "react";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Plus,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import ErrorNote from "../../lib/ErrorNote";
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

  const step = (done: boolean, next: boolean, label: string) => (
    <span
      className={cn(
        "rounded-md px-2 py-1 text-xs font-medium",
        done && "bg-success/15 text-success",
        !done && next && "bg-accent/15 text-accent-foreground",
        !done && !next && "text-muted-foreground",
      )}
    >
      {done ? "✅" : "⬜"} {label}
    </span>
  );

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="ghost" size="icon" onClick={onBack} aria-label="Back">
            <ArrowLeft aria-hidden="true" />
          </Button>
          <h1 className="text-xl font-semibold tracking-tight">{w.name}</h1>
          <Badge variant="secondary">{w.env}</Badge>
          {w.branch && (
            <Badge className="border-transparent bg-accent/15 text-accent-foreground">
              {w.branch}
            </Badge>
          )}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={openAddPkg}>
            <Plus aria-hidden="true" />
            Add package
          </Button>
          <Button variant="outline" onClick={doPull}>
            <ArrowDown aria-hidden="true" />
            Pull from Cloud
          </Button>
          <Button onClick={() => setShowPush(true)}>
            <ArrowUp aria-hidden="true" />
            Push to Cloud
          </Button>
          <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
        </div>
      </div>

      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
      {error && <ErrorNote error={error} />}

      {showGuidance && (
        <div className="grid gap-3 rounded-lg border bg-card p-4">
          <div className="grid gap-0.5">
            <strong className="text-sm">
              {hasPackages ? "Almost there." : "Your workspace is ready — but empty."}
            </strong>
            <span className="text-sm text-muted-foreground">
              {hasPackages
                ? "Publish it to GitHub so your work is versioned and shareable."
                : "Add the Creatio packages you want to version-control — only the ones you pick get downloaded."}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {step(true, false, "Workspace")}
            <span className="text-muted-foreground">→</span>
            {step(hasPackages, !hasPackages, "Packages")}
            <span className="text-muted-foreground">→</span>
            {step(hasRepo, hasPackages && !hasRepo, "GitHub repo")}
            <span className="text-muted-foreground">→</span>
            {step(isPushed, false, "Pushed")}
          </div>
          <div className="flex gap-2">
            {!hasPackages && (
              <Button size="sm" onClick={openAddPkg}>
                <Plus aria-hidden="true" />
                Add packages
              </Button>
            )}
            {hasPackages && !hasRepo && (
              <Button size="sm" onClick={() => setShowCreateRepo(true)}>Create GitHub repo</Button>
            )}
          </div>
        </div>
      )}

      <Dialog open={showAddPkg} onOpenChange={setShowAddPkg}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add package from {w.env}</DialogTitle>
            <DialogDescription>
              Pick a package to include in this workspace. It gets added to the selection and its
              source is pulled in as a Git change.
            </DialogDescription>
          </DialogHeader>
          <Input
            autoFocus
            placeholder="Filter packages…"
            value={pkgFilter}
            onChange={(e) => setPkgFilter(e.target.value)}
          />
          <div className="max-h-72 overflow-y-auto rounded-lg border">
            {pkgLoading ? (
              <p className="p-4 text-sm text-muted-foreground">Loading packages…</p>
            ) : filteredPkgs.length === 0 ? (
              <p className="p-4 text-sm text-muted-foreground">No packages match.</p>
            ) : (
              filteredPkgs.map((p) => (
                <button
                  key={p.name}
                  className="flex w-full items-center justify-between gap-3 border-b px-3 py-2 text-left text-sm last:border-b-0 hover:bg-accent/10"
                  onClick={() => doAddPackage(p.name)}
                >
                  <span className="font-medium">{p.name}</span>
                  <span className="text-xs text-muted-foreground">
                    {p.maintainer} · {p.version}
                  </span>
                </button>
              ))
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowAddPkg(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={showCreateRepo} onOpenChange={setShowCreateRepo}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create GitHub repository</DialogTitle>
            <DialogDescription>
              Creates the repository on your signed-in GitHub account, wires it as{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">origin</code>, and
              pushes the current commit. Requires the GitHub CLI signed in (Settings → GitHub).
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-2">
            <Label htmlFor="repo-name">Repository name</Label>
            <Input
              id="repo-name"
              value={repoName}
              onChange={(e) => setRepoName(e.target.value)}
              placeholder="my-workspace"
              autoFocus
            />
          </div>
          <Label className="flex items-center gap-2 font-normal">
            <Checkbox checked={repoPrivate} onCheckedChange={(c) => setRepoPrivate(c === true)} />
            Private repository (recommended)
          </Label>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCreateRepo(false)}>Cancel</Button>
            <Button onClick={doCreateRepo} disabled={!repoName.trim()}>Create &amp; push</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {drift && (
        <div className="grid gap-3 rounded-lg border border-warning/40 bg-warning/10 p-4">
          <p className="flex items-center gap-2 text-sm">
            <TriangleAlert className="size-4 text-warning" aria-hidden="true" />
            {drift}
          </p>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => setDrift("")}>Cancel</Button>
            <Button variant="outline" size="sm" onClick={() => { setDrift(""); doPull(); }}>
              Pull first
            </Button>
            <Button size="sm" onClick={() => doPushCloud(true)}>Push anyway</Button>
          </div>
        </div>
      )}

      <Dialog open={showPush} onOpenChange={setShowPush}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Push to {w.env}?</DialogTitle>
            <DialogDescription>
              Packs this workspace and installs it into the environment. The server compiles the
              configuration — expect several minutes. The job can't be safely cancelled once
              installation starts.
            </DialogDescription>
          </DialogHeader>
          <Label className="flex items-center gap-2 font-normal">
            <Checkbox checked={!skipBackup} onCheckedChange={(c) => setSkipBackup(!c)} />
            Create a backup on the environment first (recommended)
          </Label>
          {changes.length > 0 && (
            <p className="text-sm text-destructive">
              Note: {changes.length} uncommitted change(s) will be pushed as-is. Consider committing
              first.
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowPush(false)}>Cancel</Button>
            <Button onClick={() => doPushCloud(false)}>Push to Cloud</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Tabs value={tab} onValueChange={(v) => setTab(v as "changes" | "history")}>
        <TabsList>
          <TabsTrigger value="changes">Changes ({changes.length})</TabsTrigger>
          <TabsTrigger value="history">History</TabsTrigger>
        </TabsList>
      </Tabs>

      {tab === "changes" && (
        <>
          {changes.length === 0 ? (
            <p className="text-muted-foreground">
              Working tree is clean. Pull from Cloud to fetch the latest package changes.
            </p>
          ) : (
            <div className="grid gap-3 md:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
              <div className="grid max-h-[60vh] content-start gap-1 overflow-y-auto rounded-lg border p-1">
                {changes.map((c) => {
                  const slash = c.path.lastIndexOf("/");
                  const dir = slash >= 0 ? c.path.slice(0, slash + 1) : "";
                  const base = slash >= 0 ? c.path.slice(slash + 1) : c.path;
                  return (
                    <button
                      key={c.path}
                      className={cn(
                        "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent/10",
                        selectedFile === c.path && "bg-accent/15",
                      )}
                      onClick={() => showDiff(c.path)}
                    >
                      <span className="shrink-0 font-mono text-xs font-semibold text-muted-foreground uppercase">
                        {c.status}
                      </span>
                      {/* Middle ellipsis: the folder truncates but the filename always stays visible. */}
                      <span className="flex min-w-0 flex-1 items-center" title={c.path}>
                        <span className="truncate text-muted-foreground">{dir}</span>
                        <span className="shrink-0">{base}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
              <pre className="max-h-[60vh] overflow-auto rounded-lg border bg-card p-3 font-mono text-xs">
                {selectedFile
                  ? diff.split("\n").map((l, i) => (
                      <span
                        key={i}
                        className={cn(
                          "block",
                          l.startsWith("+") && "text-success",
                          l.startsWith("-") && "text-destructive",
                          l.startsWith("@@") && "text-accent-foreground",
                        )}
                      >
                        {l + "\n"}
                      </span>
                    ))
                  : "Select a file to see its diff."}
              </pre>
            </div>
          )}
          {changes.length > 0 && (
            <div className="flex gap-2">
              <Input
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder={suggested}
                onFocus={() => !message && setMessage(suggested)}
              />
              <Button onClick={doCommit}>Commit</Button>
            </div>
          )}
        </>
      )}

      {tab === "history" && (
        <>
          {remoteStatus && remoteStatus.behind > 0 && (
            <div className="grid gap-2 rounded-lg border border-warning/40 bg-warning/10 p-4">
              <p className="flex items-start gap-2 text-sm">
                <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
                Another contributor pushed {remoteStatus.behind} commit(s) to origin/
                {remoteStatus.branch}. Your push is blocked until you pull or rebase those changes,
                which may conflict with local work.
              </p>
              <div>
                <Button variant="outline" size="sm" onClick={checkRemote}>Check again</Button>
              </div>
            </div>
          )}
          {remoteStatus && remoteStatus.behind === 0 && remoteStatus.hasRemote && (
            <p className="text-sm text-muted-foreground">
              Remote is current
              {remoteStatus.ahead > 0 ? ` · ${remoteStatus.ahead} local commit(s) ready to push` : ""}.
            </p>
          )}
          {remoteError && (
            <div className="grid gap-2 rounded-lg border border-destructive/40 bg-destructive/10 p-4">
              <p className="text-sm">Remote check failed: {remoteError}</p>
              <div>
                <Button variant="outline" size="sm" onClick={checkRemote}>Check again</Button>
              </div>
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            <Input
              className="min-w-64 flex-1"
              value={remoteInput}
              onChange={(e) => setRemoteInput(e.target.value)}
              placeholder="git remote URL (https://github.com/you/repo.git)"
            />
            <Button variant="outline" onClick={doPush}>
              <ArrowUp aria-hidden="true" />
              Push to remote
            </Button>
            {!hasRepo && (
              <Button variant="outline" onClick={() => setShowCreateRepo(true)}>
                <Sparkles aria-hidden="true" />
                Create GitHub repo
              </Button>
            )}
          </div>
          {commits.length === 0 ? (
            <p className="text-muted-foreground">No commits yet.</p>
          ) : (
            <div className="grid gap-2 rounded-lg border p-2">
              {commits.map((c) => (
                <div className="flex flex-wrap items-baseline gap-2 text-sm" key={c.hash}>
                  <code className="font-mono text-xs text-muted-foreground">{c.hash}</code>
                  <span className="flex-1">{c.message}</span>
                  <span className="text-xs text-muted-foreground">{c.author} · {c.date}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
