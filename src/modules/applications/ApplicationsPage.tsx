import { useEffect, useMemo, useState } from "react";
import {
  ApplicationInfo, deployApplicationBetweenEnvironments, EnvSummary,
  listApplications, listEnvironments,
} from "../../lib/ipc";

export default function ApplicationsPage({ onShowJobs }: { onShowJobs: () => void }) {
  const [environments, setEnvironments] = useState<EnvSummary[]>([]);
  const [sourceEnv, setSourceEnv] = useState("");
  const [applications, setApplications] = useState<ApplicationInfo[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selectedApp, setSelectedApp] = useState<ApplicationInfo | null>(null);
  const [targetEnv, setTargetEnv] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [cachedAt, setCachedAt] = useState<number | null>(null);
  const [fromCache, setFromCache] = useState(false);

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvironments(list);
      const initial = list.find((environment) => environment.isActive) ?? list[0];
      if (initial) setSourceEnv(initial.name);
    }).catch((reason) => setError(String(reason)));
  }, []);

  const refresh = async (forceRefresh = true) => {
    if (!sourceEnv) return;
    setLoading(true);
    setError("");
    try {
      const result = await listApplications(sourceEnv, forceRefresh);
      setApplications(result.items);
      setCachedAt(result.cachedAt);
      setFromCache(result.fromCache);
    } catch (reason) {
      setApplications([]);
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { refresh(false); }, [sourceEnv]);

  const visible = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return applications;
    return applications.filter((application) =>
      application.name.toLowerCase().includes(query) ||
      application.code.toLowerCase().includes(query) ||
      application.version.toLowerCase().includes(query) ||
      application.description?.toLowerCase().includes(query));
  }, [applications, filter]);

  const chooseTarget = (application: ApplicationInfo) => {
    const target = environments.find((environment) => environment.name !== sourceEnv)?.name ?? "";
    setSelectedApp(application);
    setTargetEnv(target);
    setConfirmation("");
    setError("");
  };

  const closeDialog = () => {
    setSelectedApp(null);
    setTargetEnv("");
    setConfirmation("");
  };

  const deploy = async () => {
    if (!selectedApp || !targetEnv) return;
    const application = selectedApp;
    const target = targetEnv;
    closeDialog();
    setError("");
    setNotice("");
    try {
      await deployApplicationBetweenEnvironments({
        sourceEnv,
        targetEnv: target,
        appCode: application.code,
      });
      setNotice(`Deploying ${application.name} from ${sourceEnv} to ${target}. Follow the streamed output in Jobs.`);
    } catch (reason) {
      setError(String(reason));
    }
  };

  return (
    <div className="page-body">
      <div className="page-bar">
        <h1>Applications</h1>
        <div className="package-page-actions">
          <button className="ghost" onClick={onShowJobs}>Jobs</button>
          <button className="primary" onClick={() => refresh(true)} disabled={!sourceEnv || loading}>
            {loading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>

      <div className="package-toolbar">
        <label>Source environment
          <select value={sourceEnv} onChange={(event) => setSourceEnv(event.target.value)}>
            {environments.map((environment) => <option key={environment.name} value={environment.name}>
              {environment.name} {environment.isActive ? "(default)" : ""}
            </option>)}
          </select>
        </label>
        <label className="package-search">Filter
          <input value={filter} onChange={(event) => setFilter(event.target.value)}
            placeholder="Application name, code, or version" />
        </label>
        <span className="package-count">{visible.length} of {applications.length}</span>
      </div>

      <p className="hint">
        Application deployment transfers the complete Creatio application represented by its application descriptor,
        including its application packages. It is different from deploying one package from the Packages screen.
      </p>
      {cachedAt && <p className="cache-status">
        {fromCache ? "Showing saved data" : "Updated"} from {new Date(cachedAt).toLocaleString()}.
        {fromCache && " Use Refresh to check the environment for changes."}
      </p>}
      {notice && <p className="notice">{notice}</p>}
      {error && <p className="form-error">{error}</p>}

      {!loading && applications.length === 0 && !error ? <p className="empty">
        No installed applications were returned for this environment.
      </p> : <div className="application-grid">
        {visible.map((application) => <article className="application-card" key={application.id || application.code}>
          <div className="application-card-head">
            <div>
              <h3>{application.name || application.code}</h3>
              <code>{application.code}</code>
            </div>
            <span className="pill accent">{application.version || "no version"}</span>
          </div>
          <p>{application.description || "No application description."}</p>
          <div className="application-card-actions">
            <button className="primary" onClick={() => chooseTarget(application)}>
              Deploy to environment…
            </button>
          </div>
        </article>)}
      </div>}

      {selectedApp && <div className="dialog-backdrop" onClick={closeDialog}>
        <div className="dialog" onClick={(event) => event.stopPropagation()}>
          <h2>Deploy {selectedApp.name || selectedApp.code}</h2>
          <p className="hint">
            Application <strong>{selectedApp.code}</strong> version <strong>{selectedApp.version || "unspecified"}</strong>
            {" "}will be transferred from <strong>{sourceEnv}</strong> and installed into the target.
          </p>
          <p className="form-error">
            This can update multiple packages and start target-side installation or compilation.
            It cannot be safely cancelled after deployment begins.
          </p>
          <label>Target environment
            <select value={targetEnv} onChange={(event) => {
              setTargetEnv(event.target.value);
              setConfirmation("");
            }}>
              {environments.filter((environment) => environment.name !== sourceEnv).map((environment) =>
                <option key={environment.name} value={environment.name}>
                  {environment.name} — {environment.uri}
                </option>)}
            </select>
          </label>
          <label>Type <strong>{targetEnv || "the target environment"}</strong> to confirm
            <input value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoFocus />
          </label>
          <div className="dialog-actions">
            <button className="ghost" onClick={closeDialog}>Cancel</button>
            <button className="danger" disabled={!targetEnv || confirmation !== targetEnv} onClick={deploy}>
              Deploy application
            </button>
          </div>
        </div>
      </div>}
    </div>
  );
}
