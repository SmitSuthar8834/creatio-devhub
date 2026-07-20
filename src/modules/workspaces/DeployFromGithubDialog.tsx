import ErrorNote from "../../lib/ErrorNote";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
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
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <h2>Deploy from GitHub</h2>
        <p className="hint">
          Clone a repository at a chosen branch and install it into an environment — for example to
          restore a broken environment from known-good source. Only clio/DevHub workspaces (with a
          <code> .clio</code> folder) can be deployed.
        </p>

        {repoLoadError && (
          <p className="form-error">
            Couldn't list your GitHub repos ({repoLoadError}). Sign in on Settings → GitHub, or enter the
            repository manually below.
          </p>
        )}

        {!manual ? (
          <label>
            Repository
            <select value={repo} onChange={(e) => chooseRepo(e.target.value)} autoFocus>
              <option value="">Select a repository…</option>
              {repos.map((r) => (
                <option key={r.nameWithOwner} value={r.nameWithOwner}>
                  {r.nameWithOwner} {r.isPrivate ? "(private)" : ""}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <>
            <label>
              Repository (owner/name)
              <input
                value={repo}
                onChange={(e) => setRepo(e.target.value)}
                placeholder="my-org/my-repo"
                autoFocus
              />
            </label>
            <label>
              Clone URL <span className="opt">(optional — used if the account can't clone by name)</span>
              <input
                value={cloneUrl}
                onChange={(e) => setCloneUrl(e.target.value)}
                placeholder="https://github.com/my-org/my-repo.git"
              />
            </label>
          </>
        )}

        <label>
          Branch
          {!manual && branches.length > 0 ? (
            <select value={branch} onChange={(e) => setBranch(e.target.value)}>
              {branches.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </select>
          ) : (
            <input
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder={loadingBranches ? "loading branches…" : "main"}
            />
          )}
        </label>

        <label>
          Target environment
          <select value={targetEnv} onChange={(e) => setTargetEnv(e.target.value)}>
            {envs.map((e) => (
              <option key={e.name} value={e.name}>
                {e.name} {e.isActive ? "(default)" : ""}
              </option>
            ))}
          </select>
        </label>

        <label>
          Clone into folder
          <div className="path-row">
            <input
              value={destParent}
              onChange={(e) => setDestParent(e.target.value)}
              placeholder="A:\CreatioWorkspaces"
            />
            <button className="ghost" onClick={pickFolder}>
              Browse…
            </button>
          </div>
        </label>

        <label className="check-row">
          <input type="checkbox" checked={backup} onChange={(e) => setBackup(e.target.checked)} />
          Create a backup on {targetEnv || "the target"} first (recommended)
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={keepWorkspace}
            onChange={(e) => setKeepWorkspace(e.target.checked)}
          />
          Keep the clone as a workspace after deploying
        </label>

        <p className="form-error">
          This overwrites {targetEnv || "the target environment"}'s packages with the repository's
          version and starts a server-side compile that can take several minutes.
        </p>

        {error && <ErrorNote error={error} />}
        <div className="dialog-actions">
          <button className="ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" onClick={submit} disabled={busy}>
            Deploy to {targetEnv || "environment"}
          </button>
        </div>
      </div>
    </div>
  );
}
