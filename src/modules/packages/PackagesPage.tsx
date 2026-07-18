import { useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
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
    <div className="page-body">
      <div className="page-bar">
        <h1>Packages</h1>
        <div className="package-page-actions">
          <button className="ghost" onClick={chooseInstall} disabled={!env}>Install archive</button>
          <button className="ghost" onClick={onShowJobs}>Jobs</button>
          <button className="primary" onClick={() => refresh(true)} disabled={!env || loading}>
            {loading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>

      <div className="package-toolbar">
        <label>Environment
          <select value={env} onChange={(event) => setEnv(event.target.value)}>
            {envs.map((item) => <option key={item.name} value={item.name}>
              {item.name} {item.isActive ? "(default)" : ""}
            </option>)}
          </select>
        </label>
        <label className="package-search">Filter
          <input value={filter} onChange={(event) => setFilter(event.target.value)}
            placeholder="Package, version, or maintainer" />
        </label>
        <span className="package-count">{visible.length} of {packages.length}</span>
      </div>

      {cachedAt && <p className="cache-status">
        {fromCache ? "Showing saved data" : "Updated"} from {new Date(cachedAt).toLocaleString()}.
        {fromCache && " Use Refresh to check the environment for changes."}
      </p>}
      {notice && <p className="notice">{notice}</p>}
      {error && <p className="form-error">{error}</p>}

      <div className={`package-drop ${dragging ? "dragging" : ""}`} onClick={chooseInstall}>
        Drop a .zip or .gz package here to install it, or click to browse.
      </div>

      {!loading && packages.length === 0 && !error ? <p className="empty">No packages returned for this environment.</p> :
        <div className="package-table-wrap">
          <table className="package-table">
            <thead><tr><th>Package</th><th>Version</th><th>Maintainer</th><th>Actions</th></tr></thead>
            <tbody>{visible.map((pkg) => <tr key={pkg.name}>
              <td><strong>{pkg.name}</strong></td>
              <td><code>{pkg.version || "—"}</code></td>
              <td>{pkg.maintainer || "—"}</td>
              <td><div className="package-actions">
                <button onClick={() => pull(pkg)}>Pull</button>
                <button onClick={() => chooseWorkspace(pkg)}>Add to workspace</button>
                <details><summary>More</summary><div className="package-menu">
                  <button onClick={() => simpleAction(pkg, "lock")}>Lock</button>
                  <button onClick={() => simpleAction(pkg, "unlock")}>Unlock</button>
                  <button onClick={() => simpleAction(pkg, "activate")}>Activate</button>
                  <button onClick={() => simpleAction(pkg, "deactivate")}>Deactivate</button>
                  <button onClick={() => chooseDeployTarget(pkg)}>Deploy to environment…</button>
                  <button onClick={() => start({ package: pkg.name, action: "hotfix", value: "true" })}>Enable hotfix</button>
                  <button onClick={() => start({ package: pkg.name, action: "hotfix", value: "false" })}>Disable hotfix</button>
                  <button onClick={() => { setVersion(pkg.version); setDialog({ kind: "version", pkg }); }}>Set version…</button>
                  <button className="danger-text" onClick={() => setDialog({ kind: "delete", pkg })}>Delete…</button>
                </div></details>
              </div></td>
            </tr>)}</tbody>
          </table>
        </div>}

      {dialog?.kind === "install" && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Install package?</h2>
          <p className="hint package-path">{dialog.path}</p>
          <p>This installs the archive into <strong>{env}</strong> and may start a server-side compile.</p>
          <label className="check-row"><input type="checkbox" checked={!skipBackup}
            onChange={(event) => setSkipBackup(!event.target.checked)} />Create a backup first (recommended)</label>
          <div className="dialog-actions"><button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="primary" onClick={() => install(dialog.path)}>Install</button></div>
        </div>
      </div>}

      {dialog?.kind === "version" && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Set {dialog.pkg.name} version</h2>
          <p className="hint">DevHub downloads the package, updates its descriptor, and installs it back into {env}.</p>
          <label>New version<input value={version} onChange={(event) => setVersion(event.target.value)} autoFocus /></label>
          <label className="check-row"><input type="checkbox" checked={!skipBackup}
            onChange={(event) => setSkipBackup(!event.target.checked)} />Create a backup before reinstalling (recommended)</label>
          <div className="dialog-actions"><button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="primary" onClick={() => setPackageVersion(dialog.pkg)}>Set version</button></div>
        </div>
      </div>}

      {dialog?.kind === "workspace" && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Add {dialog.pkg.name} to workspace</h2>
          <p className="hint">
            The package will be appended to the workspace selection and restored from {env}.
            Its files will then appear as Git changes ready for review and commit.
          </p>
          {matchingWorkspaces.length === 0 ? <p className="form-error">
            No available workspace is registered for {env}. Create or register one first.
          </p> : <label>Workspace
            <select value={selectedWorkspace} onChange={(event) => setSelectedWorkspace(event.target.value)}>
              {matchingWorkspaces.map((workspace) => <option key={workspace.id} value={workspace.id}>
                {workspace.name} — {workspace.path}
              </option>)}
            </select>
          </label>}
          <p className="hint">
            The workspace must have a clean Git tree. DevHub will refuse the operation if unrelated changes are present.
          </p>
          <div className="dialog-actions">
            <button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="primary" disabled={!selectedWorkspace}
              onClick={() => addToWorkspace(dialog.pkg)}>Add and restore</button>
          </div>
        </div>
      </div>}

      {dialog?.kind === "deploy" && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Deploy {dialog.pkg.name}</h2>
          <p className="hint">
            DevHub downloads the package from <strong>{env}</strong> and installs it into the target environment.
            Existing target configuration may be overwritten.
          </p>
          <label>Target environment
            <select value={targetEnv} onChange={(event) => {
              setTargetEnv(event.target.value);
              setDeployConfirm("");
            }}>
              {envs.filter((environment) => environment.name !== env).map((environment) =>
                <option key={environment.name} value={environment.name}>{environment.name} — {environment.uri}</option>)}
            </select>
          </label>
          <label className="check-row"><input type="checkbox" checked={!skipBackup}
            onChange={(event) => setSkipBackup(!event.target.checked)} />Create a backup in the target first (recommended)</label>
          <label>Type <strong>{targetEnv || "the target environment"}</strong> to confirm
            <input value={deployConfirm} onChange={(event) => setDeployConfirm(event.target.value)} /></label>
          <div className="dialog-actions">
            <button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="danger" disabled={!targetEnv || deployConfirm !== targetEnv}
              onClick={() => deployPackage(dialog.pkg)}>Deploy package</button>
          </div>
        </div>
      </div>}

      {dialog?.kind === "delete" && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Delete {dialog.pkg.name}?</h2>
          <p className="form-error">This removes the package from {env} and can affect dependent configuration.</p>
          <label>Type <strong>{dialog.pkg.name}</strong> to confirm
            <input value={confirmText} onChange={(event) => setConfirmText(event.target.value)} autoFocus /></label>
          <div className="dialog-actions"><button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="danger" disabled={confirmText !== dialog.pkg.name}
              onClick={() => deletePackage(dialog.pkg)}>Delete package</button></div>
        </div>
      </div>}
    </div>
  );
}
