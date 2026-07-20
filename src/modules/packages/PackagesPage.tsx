import { useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, Upload } from "lucide-react";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import ErrorNote from "../../lib/ErrorNote";
import {
  addPackageToWorkspace, deployPackageBetweenEnvironments, EnvSummary, listEnvironments,
  listPackages, listWorkspaces, onCatalogUpdated, onJobUpdate, PackageAction, PackageInfo,
  runPackageAction, WorkspaceSummary,
} from "../../lib/ipc";

type DialogState =
  | { kind: "delete"; pkg: PackageInfo }
  | { kind: "version"; pkg: PackageInfo }
  | { kind: "install"; path: string }
  | { kind: "workspace"; pkg: PackageInfo }
  | { kind: "deploy"; pkg: PackageInfo }
  | null;

const isArchive = (path: string) => /\.(zip|gz)$/i.test(path);

export default function PackagesPage({
  onShowJobs,
  onOpenWorkspace,
}: {
  onShowJobs: () => void;
  onOpenWorkspace: (id: string) => void;
}) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [env, setEnv] = useState("");
  const [packages, setPackages] = useState<PackageInfo[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [dialog, setDialog] = useState<DialogState>(null);
  const [confirmText, setConfirmText] = useState("");
  const [version, setVersion] = useState("");
  const [skipBackup, setSkipBackup] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [matchingWorkspaces, setMatchingWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [selectedWorkspace, setSelectedWorkspace] = useState("");
  const [pendingWorkspaceJob, setPendingWorkspaceJob] = useState<{
    jobId: string;
    workspaceId: string;
  } | null>(null);
  const [targetEnv, setTargetEnv] = useState("");
  const [deployConfirm, setDeployConfirm] = useState("");
  const [cachedAt, setCachedAt] = useState<number | null>(null);
  const [fromCache, setFromCache] = useState(false);

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvs(list);
      const initial = list.find((item) => item.isActive) ?? list[0];
      if (initial) setEnv(initial.name);
    }).catch((e) => setError(String(e)));
  }, []);

  const refresh = async (forceRefresh = true) => {
    if (!env) return;
    setLoading(true);
    setError("");
    try {
      const result = await listPackages(env, forceRefresh);
      setPackages(result.items);
      setCachedAt(result.cachedAt);
      setFromCache(result.fromCache);
    } catch (e) {
      setPackages([]);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { refresh(false); }, [env]);

  // Reload from cache when the background prefetch freshens this environment.
  useEffect(() => {
    const un = onCatalogUpdated((updated) => { if (updated === env) refresh(false); });
    return () => { un.then((f) => f()); };
  }, [env]);

  useEffect(() => {
    const unlisten = getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") setDragging(true);
      else if (event.payload.type === "leave") setDragging(false);
      else if (event.payload.type === "drop") {
        setDragging(false);
        const path = event.payload.paths.find(isArchive);
        if (path) {
          setSkipBackup(false);
          setDialog({ kind: "install", path });
        } else setError("Drop a Creatio .zip or .gz package archive.");
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const unlisten = onJobUpdate((job) => {
      if (pendingWorkspaceJob && job.id === pendingWorkspaceJob.jobId &&
          ["succeeded", "failed", "cancelled"].includes(job.status)) {
        if (job.status === "succeeded") {
          onOpenWorkspace(pendingWorkspaceJob.workspaceId);
        } else if (job.status === "cancelled") {
          setNotice("Adding the package was cancelled before completion.");
        } else {
          setError("Adding the package failed. Open Jobs to review the clio output.");
        }
        setPendingWorkspaceJob(null);
      }
      if (job.env === env && job.status === "succeeded" &&
          ["install-package", "delete-package", "set-package-version"].includes(job.kind)) {
        refresh(true);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [env, pendingWorkspaceJob, onOpenWorkspace]);

  const visible = useMemo(() => {
    const query = filter.trim().toLowerCase();
    return query ? packages.filter((pkg) =>
      pkg.name.toLowerCase().includes(query) ||
      pkg.version.toLowerCase().includes(query) ||
      pkg.maintainer.toLowerCase().includes(query)) : packages;
  }, [filter, packages]);

  const start = async (opts: {
    package: string; action: PackageAction; path?: string; value?: string; skipBackup?: boolean;
  }) => {
    setError("");
    setNotice("");
    try {
      await runPackageAction({ env, ...opts });
      setNotice("Job started. Follow the live output on the Jobs screen.");
    } catch (e) {
      setError(String(e));
    }
  };

  const pull = async (pkg: PackageInfo) => {
    const destination = await open({ directory: true, title: `Download ${pkg.name} into…` });
    if (typeof destination === "string") await start({ package: pkg.name, action: "pull", path: destination });
  };

  const simpleAction = async (pkg: PackageInfo, action: PackageAction) => {
    if (confirm(`${action[0].toUpperCase() + action.slice(1)} "${pkg.name}" on ${env}?`)) {
      await start({ package: pkg.name, action });
    }
  };

  const chooseInstall = async () => {
    const path = await open({
      multiple: false, title: "Choose Creatio package",
      filters: [{ name: "Creatio package", extensions: ["zip", "gz"] }],
    });
    if (typeof path === "string") {
      setSkipBackup(false);
      setDialog({ kind: "install", path });
    }
  };

  const chooseWorkspace = async (pkg: PackageInfo) => {
    setError("");
    try {
      const matches = (await listWorkspaces()).filter((workspace) =>
        workspace.env === env && workspace.exists);
      setMatchingWorkspaces(matches);
      setSelectedWorkspace(matches[0]?.id ?? "");
      setDialog({ kind: "workspace", pkg });
    } catch (e) {
      setError(String(e));
    }
  };

  const addToWorkspace = async (pkg: PackageInfo) => {
    if (!selectedWorkspace) return;
    const workspaceId = selectedWorkspace;
    closeDialog();
    setError("");
    setNotice("");
    try {
      const jobId = await addPackageToWorkspace(workspaceId, pkg.name);
      setPendingWorkspaceJob({ jobId, workspaceId });
      setNotice(`Adding ${pkg.name} to the workspace. This page will open its Changes tab when the restore finishes.`);
    } catch (e) {
      setError(String(e));
    }
  };

  const closeDialog = () => {
    setDialog(null);
    setConfirmText("");
    setVersion("");
    setSkipBackup(false);
    setTargetEnv("");
    setDeployConfirm("");
  };

  const install = async (path: string) => {
    const backup = skipBackup;
    closeDialog();
    await start({ package: "archive", action: "push", path, skipBackup: backup });
  };

  const setPackageVersion = async (pkg: PackageInfo) => {
    const next = version.trim();
    const backup = skipBackup;
    closeDialog();
    await start({ package: pkg.name, action: "version", value: next, skipBackup: backup });
  };

  const deletePackage = async (pkg: PackageInfo) => {
    closeDialog();
    await start({ package: pkg.name, action: "delete" });
  };

  const chooseDeployTarget = (pkg: PackageInfo) => {
    const firstTarget = envs.find((environment) => environment.name !== env)?.name ?? "";
    setTargetEnv(firstTarget);
    setDeployConfirm("");
    setSkipBackup(false);
    setDialog({ kind: "deploy", pkg });
  };

  const deployPackage = async (pkg: PackageInfo) => {
    const target = targetEnv;
    const backup = skipBackup;
    closeDialog();
    setError("");
    setNotice("");
    try {
      await deployPackageBetweenEnvironments({
        sourceEnv: env,
        targetEnv: target,
        package: pkg.name,
        skipBackup: backup,
      });
      setNotice(`Deploying ${pkg.name} from ${env} to ${target}. Follow the job output in Jobs.`);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Packages</h1>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={chooseInstall} disabled={!env}>Install archive</Button>
          <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
          <Button onClick={() => refresh(true)} disabled={!env || loading}>
            {loading ? "Loading…" : "Refresh"}
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-end gap-3">
        <div className="grid min-w-56 gap-2">
          <Label htmlFor="pkg-env">Environment</Label>
          <Select value={env} onValueChange={setEnv}>
            <SelectTrigger id="pkg-env" className="w-full">
              <SelectValue placeholder="Select an environment" />
            </SelectTrigger>
            <SelectContent>
              {envs.map((item) => (
                <SelectItem key={item.name} value={item.name}>
                  {item.name} {item.isActive ? "(default)" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="grid min-w-64 flex-1 gap-2">
          <Label htmlFor="pkg-filter">Filter</Label>
          <Input
            id="pkg-filter"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Package, version, or maintainer"
          />
        </div>
        <span className="pb-2.5 text-sm text-muted-foreground">
          {visible.length} of {packages.length}
        </span>
      </div>

      {cachedAt && (
        <p className="text-xs text-muted-foreground">
          {fromCache ? "Showing saved data" : "Updated"} from {new Date(cachedAt).toLocaleString()}.
          {fromCache && " Use Refresh to check the environment for changes."}
        </p>
      )}
      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
      {error && <ErrorNote error={error} />}

      <button
        onClick={chooseInstall}
        className={cn(
          "flex items-center justify-center gap-2 rounded-lg border border-dashed p-4 text-sm text-muted-foreground transition-colors hover:border-primary hover:text-foreground",
          dragging && "border-primary bg-accent/10 text-foreground",
        )}
      >
        <Upload className="size-4" aria-hidden="true" />
        Drop a .zip or .gz package here to install it, or click to browse.
      </button>

      {!loading && packages.length === 0 && !error ? (
        <p className="text-muted-foreground">No packages returned for this environment.</p>
      ) : (
        <div className="overflow-x-auto rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Package</TableHead>
                <TableHead>Version</TableHead>
                <TableHead>Maintainer</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visible.map((pkg) => (
                <TableRow key={pkg.name}>
                  <TableCell className="font-medium">{pkg.name}</TableCell>
                  <TableCell>
                    <code className="font-mono text-xs">{pkg.version || "—"}</code>
                  </TableCell>
                  <TableCell>{pkg.maintainer || "—"}</TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-2">
                      <Button size="sm" variant="outline" onClick={() => pull(pkg)}>Pull</Button>
                      <Button size="sm" variant="outline" onClick={() => chooseWorkspace(pkg)}>
                        Add to workspace
                      </Button>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button size="sm" variant="ghost">
                            More <ChevronDown aria-hidden="true" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem onClick={() => simpleAction(pkg, "lock")}>Lock</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => simpleAction(pkg, "unlock")}>Unlock</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => simpleAction(pkg, "activate")}>Activate</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => simpleAction(pkg, "deactivate")}>Deactivate</DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem onClick={() => chooseDeployTarget(pkg)}>
                            Deploy to environment…
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => start({ package: pkg.name, action: "hotfix", value: "true" })}>
                            Enable hotfix
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => start({ package: pkg.name, action: "hotfix", value: "false" })}>
                            Disable hotfix
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => { setVersion(pkg.version); setDialog({ kind: "version", pkg }); }}>
                            Set version…
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            variant="destructive"
                            onClick={() => { setConfirmText(""); setDialog({ kind: "delete", pkg }); }}
                          >
                            Delete…
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <Dialog open={dialog?.kind === "install"} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {dialog?.kind === "install" && (
            <>
              <DialogHeader>
                <DialogTitle>Install package?</DialogTitle>
                <DialogDescription className="font-mono text-xs break-all">
                  {dialog.path}
                </DialogDescription>
              </DialogHeader>
              <p className="text-sm">
                This installs the archive into <strong>{env}</strong> and may start a server-side compile.
              </p>
              <Label className="flex items-center gap-2 font-normal">
                <Checkbox
                  checked={!skipBackup}
                  onCheckedChange={(checked) => setSkipBackup(!checked)}
                />
                Create a backup first (recommended)
              </Label>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button onClick={() => install(dialog.path)}>Install</Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={dialog?.kind === "version"} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {dialog?.kind === "version" && (
            <>
              <DialogHeader>
                <DialogTitle>Set {dialog.pkg.name} version</DialogTitle>
                <DialogDescription>
                  DevHub downloads the package, updates its descriptor, and installs it back into {env}.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-2">
                <Label htmlFor="pkg-version">New version</Label>
                <Input
                  id="pkg-version"
                  value={version}
                  onChange={(event) => setVersion(event.target.value)}
                  autoFocus
                />
              </div>
              <Label className="flex items-center gap-2 font-normal">
                <Checkbox
                  checked={!skipBackup}
                  onCheckedChange={(checked) => setSkipBackup(!checked)}
                />
                Create a backup before reinstalling (recommended)
              </Label>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button onClick={() => setPackageVersion(dialog.pkg)}>Set version</Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={dialog?.kind === "workspace"} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {dialog?.kind === "workspace" && (
            <>
              <DialogHeader>
                <DialogTitle>Add {dialog.pkg.name} to workspace</DialogTitle>
                <DialogDescription>
                  The package will be appended to the workspace selection and restored from {env}.
                  Its files will then appear as Git changes ready for review and commit.
                </DialogDescription>
              </DialogHeader>
              {matchingWorkspaces.length === 0 ? (
                <p className="text-sm text-destructive">
                  No available workspace is registered for {env}. Create or register one first.
                </p>
              ) : (
                <div className="grid gap-2">
                  <Label htmlFor="pkg-workspace">Workspace</Label>
                  <Select value={selectedWorkspace} onValueChange={setSelectedWorkspace}>
                    <SelectTrigger id="pkg-workspace" className="w-full">
                      <SelectValue placeholder="Select a workspace" />
                    </SelectTrigger>
                    <SelectContent>
                      {matchingWorkspaces.map((workspace) => (
                        <SelectItem key={workspace.id} value={workspace.id}>
                          {workspace.name} — {workspace.path}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
              <p className="text-sm text-muted-foreground">
                The workspace must have a clean Git tree. DevHub will refuse the operation if
                unrelated changes are present.
              </p>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button disabled={!selectedWorkspace} onClick={() => addToWorkspace(dialog.pkg)}>
                  Add and restore
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={dialog?.kind === "deploy"} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {dialog?.kind === "deploy" && (
            <>
              <DialogHeader>
                <DialogTitle>Deploy {dialog.pkg.name}</DialogTitle>
                <DialogDescription>
                  DevHub downloads the package from <strong>{env}</strong> and installs it into the
                  target environment. Existing target configuration may be overwritten.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-2">
                <Label htmlFor="pkg-target">Target environment</Label>
                <Select
                  value={targetEnv}
                  onValueChange={(value) => { setTargetEnv(value); setDeployConfirm(""); }}
                >
                  <SelectTrigger id="pkg-target" className="w-full">
                    <SelectValue placeholder="Select a target" />
                  </SelectTrigger>
                  <SelectContent>
                    {envs.filter((environment) => environment.name !== env).map((environment) => (
                      <SelectItem key={environment.name} value={environment.name}>
                        {environment.name} — {environment.uri}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <Label className="flex items-center gap-2 font-normal">
                <Checkbox
                  checked={!skipBackup}
                  onCheckedChange={(checked) => setSkipBackup(!checked)}
                />
                Create a backup in the target first (recommended)
              </Label>
              <div className="grid gap-2">
                <Label htmlFor="pkg-deploy-confirm">
                  Type <strong>{targetEnv || "the target environment"}</strong> to confirm
                </Label>
                <Input
                  id="pkg-deploy-confirm"
                  value={deployConfirm}
                  onChange={(event) => setDeployConfirm(event.target.value)}
                />
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button
                  variant="destructive"
                  disabled={!targetEnv || deployConfirm !== targetEnv}
                  onClick={() => deployPackage(dialog.pkg)}
                >
                  Deploy package
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={dialog?.kind === "delete"} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {dialog?.kind === "delete" && (
            <>
              <DialogHeader>
                <DialogTitle>Delete {dialog.pkg.name}?</DialogTitle>
                <DialogDescription className="text-destructive">
                  This removes the package from {env} and can affect dependent configuration.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-2">
                <Label htmlFor="pkg-delete-confirm">
                  Type <strong>{dialog.pkg.name}</strong> to confirm
                </Label>
                <Input
                  id="pkg-delete-confirm"
                  value={confirmText}
                  onChange={(event) => setConfirmText(event.target.value)}
                  autoFocus
                />
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button
                  variant="destructive"
                  disabled={confirmText !== dialog.pkg.name}
                  onClick={() => deletePackage(dialog.pkg)}
                >
                  Delete package
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
