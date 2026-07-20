import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import ErrorNote from "../../lib/ErrorNote";
import {
  deployFromGithub,
  EnvSummary,
  GithubRepo,
  listEnvironments,
  listGithubRepos,
  listRepoBranches,
} from "../../lib/ipc";

interface Props {
  onClose: () => void;
  onStarted: () => void;
}

/**
 * Deploy a Creatio workspace straight from a GitHub repo into an environment —
 * e.g. to restore a broken environment from known-good source. Clones/refreshes
 * the repo at the chosen branch, then push-workspace installs it.
 */
export default function DeployFromGithubDialog({ onClose, onStarted }: Props) {
  const [repos, setRepos] = useState<GithubRepo[]>([]);
  const [repoLoadError, setRepoLoadError] = useState("");
  const [manual, setManual] = useState(false);

  const [repo, setRepo] = useState(""); // owner/name
  const [cloneUrl, setCloneUrl] = useState("");
  const [branches, setBranches] = useState<string[]>([]);
  const [branch, setBranch] = useState("");
  const [loadingBranches, setLoadingBranches] = useState(false);

  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [targetEnv, setTargetEnv] = useState("");
  const [destParent, setDestParent] = useState("");
  const [keepWorkspace, setKeepWorkspace] = useState(true);
  const [backup, setBackup] = useState(true);

  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listGithubRepos()
      .then((list) => {
        setRepos(list);
        if (list.length === 0) setManual(true);
      })
      .catch((e) => {
        setRepoLoadError(String(e));
        setManual(true);
      });
    listEnvironments().then((list) => {
      setEnvs(list);
      const active = list.find((e) => e.isActive) ?? list[0];
      if (active) setTargetEnv(active.name);
    });
  }, []);

  // When a repo is picked from the list, seed its clone URL + branches.
  const chooseRepo = (nameWithOwner: string) => {
    setRepo(nameWithOwner);
    const found = repos.find((r) => r.nameWithOwner === nameWithOwner);
    setCloneUrl(found?.url ?? "");
    setBranch(found?.defaultBranch ?? "");
    setBranches([]);
    if (!nameWithOwner) return;
    setLoadingBranches(true);
    listRepoBranches(nameWithOwner)
      .then(setBranches)
      .catch(() => setBranches([]))
      .finally(() => setLoadingBranches(false));
  };

  const pickFolder = async () => {
    const dir = await open({ directory: true });
    if (typeof dir === "string") setDestParent(dir);
  };

  const submit = async () => {
    setError("");
    if (!repo.trim() || !repo.includes("/")) {
      setError("Choose a repository (owner/name).");
      return;
    }
    if (!branch.trim()) {
      setError("Choose a branch to deploy.");
      return;
    }
    if (!targetEnv) {
      setError("Choose a target environment.");
      return;
    }
    if (!destParent.trim()) {
      setError("Choose a local folder to clone into.");
      return;
    }
    setBusy(true);
    try {
      await deployFromGithub({
        repo: repo.trim(),
        cloneUrl: cloneUrl.trim(),
        branch: branch.trim(),
        destParent: destParent.trim(),
        targetEnv,
        skipBackup: !backup,
        register: keepWorkspace,
      });
      onStarted();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Deploy from GitHub</DialogTitle>
          <DialogDescription>
            Clone a repository at a chosen branch and install it into an environment — for example
            to restore a broken environment from known-good source. Only clio/DevHub workspaces
            (with a <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">.clio</code>{" "}
            folder) can be deployed.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          {repoLoadError && (
            <p className="text-sm text-destructive">
              Couldn't list your GitHub repos ({repoLoadError}). Sign in on Settings → GitHub, or
              enter the repository manually below.
            </p>
          )}

          {!manual ? (
            <div className="grid gap-2">
              <Label htmlFor="gh-repo">Repository</Label>
              <Select value={repo} onValueChange={chooseRepo}>
                <SelectTrigger id="gh-repo" className="w-full">
                  <SelectValue placeholder="Select a repository…" />
                </SelectTrigger>
                <SelectContent>
                  {repos.map((r) => (
                    <SelectItem key={r.nameWithOwner} value={r.nameWithOwner}>
                      {r.nameWithOwner} {r.isPrivate ? "(private)" : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : (
            <>
              <div className="grid gap-2">
                <Label htmlFor="gh-repo-manual">Repository (owner/name)</Label>
                <Input
                  id="gh-repo-manual"
                  value={repo}
                  onChange={(e) => setRepo(e.target.value)}
                  placeholder="my-org/my-repo"
                  autoFocus
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="gh-clone-url">
                  Clone URL{" "}
                  <span className="text-muted-foreground">
                    (optional — used if the account can't clone by name)
                  </span>
                </Label>
                <Input
                  id="gh-clone-url"
                  value={cloneUrl}
                  onChange={(e) => setCloneUrl(e.target.value)}
                  placeholder="https://github.com/my-org/my-repo.git"
                />
              </div>
            </>
          )}

          <div className="grid gap-2">
            <Label htmlFor="gh-branch">Branch</Label>
            {!manual && branches.length > 0 ? (
              <Select value={branch} onValueChange={setBranch}>
                <SelectTrigger id="gh-branch" className="w-full">
                  <SelectValue placeholder="Select a branch" />
                </SelectTrigger>
                <SelectContent>
                  {branches.map((b) => (
                    <SelectItem key={b} value={b}>{b}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                id="gh-branch"
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
                placeholder={loadingBranches ? "loading branches…" : "main"}
              />
            )}
          </div>

          <div className="grid gap-2">
            <Label htmlFor="gh-target">Target environment</Label>
            <Select value={targetEnv} onValueChange={setTargetEnv}>
              <SelectTrigger id="gh-target" className="w-full">
                <SelectValue placeholder="Select an environment" />
              </SelectTrigger>
              <SelectContent>
                {envs.map((e) => (
                  <SelectItem key={e.name} value={e.name}>
                    {e.name} {e.isActive ? "(default)" : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="gh-dest">Clone into folder</Label>
            <div className="flex gap-2">
              <Input
                id="gh-dest"
                value={destParent}
                onChange={(e) => setDestParent(e.target.value)}
                placeholder="A:\CreatioWorkspaces"
              />
              <Button variant="outline" onClick={pickFolder}>Browse…</Button>
            </div>
          </div>

          <Label className="flex items-center gap-2 font-normal">
            <Checkbox checked={backup} onCheckedChange={(c) => setBackup(c === true)} />
            Create a backup on {targetEnv || "the target"} first (recommended)
          </Label>
          <Label className="flex items-center gap-2 font-normal">
            <Checkbox
              checked={keepWorkspace}
              onCheckedChange={(c) => setKeepWorkspace(c === true)}
            />
            Keep the clone as a workspace after deploying
          </Label>

          <p className="text-sm text-destructive">
            This overwrites {targetEnv || "the target environment"}'s packages with the repository's
            version and starts a server-side compile that can take several minutes.
          </p>

          {error && <ErrorNote error={error} />}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button onClick={submit} disabled={busy}>
            Deploy to {targetEnv || "environment"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
