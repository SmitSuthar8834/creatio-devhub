import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { createWorkspaceFlow, EnvSummary, listEnvironments, registerWorkspace } from "../../lib/ipc";

interface Props {
  onClose: () => void;
  onStarted: () => void;
}

export default function NewWorkspaceWizard({ onClose, onStarted }: Props) {
  const [mode, setMode] = useState<"create" | "existing">("create");
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [env, setEnv] = useState("");
  const [name, setName] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [appCode, setAppCode] = useState("");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [startEmpty, setStartEmpty] = useState(true);
  const [existingPath, setExistingPath] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvs(list);
      const active = list.find((e) => e.isActive) ?? list[0];
      if (active) setEnv(active.name);
    });
  }, []);

  const pickFolder = async (set: (v: string) => void) => {
    const dir = await open({ directory: true });
    if (typeof dir === "string") set(dir);
  };

  const submit = async () => {
    setError("");
    setBusy(true);
    try {
      if (mode === "create") {
        if (!name.trim() || !parentDir.trim() || !env) {
          setError("Name, folder and environment are required.");
          return;
        }
        await createWorkspaceFlow({
          name: name.trim(),
          parentDir: parentDir.trim(),
          env,
          appCode: appCode.trim() || undefined,
          remoteUrl: remoteUrl.trim() || undefined,
          skipRestore: startEmpty,
        });
      } else {
        if (!existingPath.trim() || !env) {
          setError("Folder and environment are required.");
          return;
        }
        await registerWorkspace(existingPath.trim(), env);
      }
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
        <h2>New workspace</h2>
        <div className="auth-toggle">
          <button className={mode === "create" ? "on" : ""} onClick={() => setMode("create")}>
            Pull from environment
          </button>
          <button className={mode === "existing" ? "on" : ""} onClick={() => setMode("existing")}>
            Open existing folder
          </button>
        </div>

        <label>
          Environment
          <select value={env} onChange={(e) => setEnv(e.target.value)}>
            {envs.map((e) => (
              <option key={e.name} value={e.name}>
                {e.name} {e.isActive ? "(default)" : ""}
              </option>
            ))}
          </select>
        </label>

        {mode === "create" ? (
          <>
            <label>
              Workspace name
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-app-workspace" autoFocus />
            </label>
            <label>
              Create inside folder
              <div className="path-row">
                <input value={parentDir} onChange={(e) => setParentDir(e.target.value)} placeholder="A:\CreatioWorkspaces" />
                <button className="ghost" onClick={() => pickFolder(setParentDir)}>
                  Browse…
                </button>
              </div>
            </label>
            <label>
              App code <span className="opt">(optional — limits the workspace to one app)</span>
              <input value={appCode} onChange={(e) => setAppCode(e.target.value)} placeholder="MyAppCode" />
            </label>

            <fieldset className="choice-group">
              <legend>Contents</legend>
              <label className="choice-row">
                <input type="radio" checked={startEmpty} onChange={() => setStartEmpty(true)} />
                <span>
                  <strong>Start empty</strong> — scaffold only, pick packages to add afterwards <span className="opt">(recommended)</span>
                </span>
              </label>
              <label className="choice-row">
                <input type="radio" checked={!startEmpty} onChange={() => setStartEmpty(false)} />
                <span>
                  <strong>Pull everything now</strong> — download all editable packages from the environment
                </span>
              </label>
            </fieldset>

            <label>
              Git remote URL <span className="opt">(optional — you can also create a GitHub repo later from the workspace)</span>
              <input
                value={remoteUrl}
                onChange={(e) => setRemoteUrl(e.target.value)}
                placeholder="https://github.com/you/repo.git"
              />
            </label>
            <p className="hint">
              {startEmpty
                ? "Creates an empty clio workspace and an initial git commit — no packages downloaded. Add packages and create a GitHub repo from the workspace screen. Watch progress on the Jobs screen."
                : "Runs create-workspace + restore-workspace against the environment, then initializes git with an initial commit. Watch progress on the Jobs screen."}
            </p>
          </>
        ) : (
          <>
            <label>
              Workspace folder
              <div className="path-row">
                <input value={existingPath} onChange={(e) => setExistingPath(e.target.value)} placeholder="C:\path\to\cloned-workspace" />
                <button className="ghost" onClick={() => pickFolder(setExistingPath)}>
                  Browse…
                </button>
              </div>
            </label>
            <p className="hint">For a cloned repo or a workspace created outside DevHub. The folder must contain .clio.</p>
          </>
        )}

        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button className="ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" onClick={submit} disabled={busy}>
            {mode === "create" ? "Create workspace" : "Add workspace"}
          </button>
        </div>
      </div>
    </div>
  );
}
