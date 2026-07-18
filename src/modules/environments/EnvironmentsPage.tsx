import { useEffect, useState } from "react";
import { EnvSummary, JobInfo, listEnvironments, onJobUpdate, runClioJob } from "../../lib/ipc";
import AddEnvironmentDialog from "./AddEnvironmentDialog";

type EnvStatus = "unknown" | "checking" | "online" | "offline";

export default function EnvironmentsPage() {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [status, setStatus] = useState<Record<string, EnvStatus>>({});
  const [pingJobs, setPingJobs] = useState<Record<string, string>>({}); // jobId -> env
  const [showAdd, setShowAdd] = useState(false);

  const refresh = () => listEnvironments().then(setEnvs).catch(console.error);

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    const un = onJobUpdate((job: JobInfo) => {
      const envName = pingJobs[job.id];
      if (envName && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        setStatus((s) => ({
          ...s,
          [envName]: job.status === "succeeded" ? "online" : job.status === "failed" ? "offline" : "unknown",
        }));
      }
      if (job.kind === "reg-web-app" && job.status === "succeeded") {
        refresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [pingJobs]);

  const ping = async (env: EnvSummary) => {
    setStatus((s) => ({ ...s, [env.name]: "checking" }));
    const jobId = await runClioJob("ping-app", ["ping", "-e", env.name], env.name);
    setPingJobs((p) => ({ ...p, [jobId]: env.name }));
  };

  const open = (env: EnvSummary) => runClioJob("open-web-app", ["open", "-e", env.name], env.name);

  const installGate = (env: EnvSummary) =>
    runClioJob("install-gate", ["install-gate", "-e", env.name], env.name);

  const statusPill = (s: EnvStatus) => {
    switch (s) {
      case "online":
        return <span className="pill ok">online</span>;
      case "offline":
        return <span className="pill bad">unreachable</span>;
      case "checking":
        return <span className="pill">checking…</span>;
      default:
        return <span className="pill dim">not checked</span>;
    }
  };

  return (
    <div className="page-body">
      <div className="page-bar">
        <h1>Environments</h1>
        <button className="primary" onClick={() => setShowAdd(true)}>
          + Add environment
        </button>
      </div>
      {envs.length === 0 && (
        <p className="empty">No environments registered yet. Add one to get started.</p>
      )}
      <div className="env-grid">
        {envs.map((env) => (
          <div className="env-card" key={env.name}>
            <div className="env-head">
              <strong>{env.name}</strong>
              {env.isActive && <span className="pill accent">default</span>}
              {statusPill(status[env.name] ?? "unknown")}
            </div>
            <div className="env-uri" title={env.uri}>
              {env.uri}
            </div>
            <div className="env-meta">
              <span className={`pill ${env.authKind === "oauth" ? "ok" : "warn"}`}>
                {env.authKind === "oauth" ? "OAuth" : env.authKind === "password" ? "password auth" : "no auth"}
              </span>
              {env.developerMode && <span className="pill dim">dev mode</span>}
            </div>
            <div className="env-actions">
              <button onClick={() => ping(env)}>Ping</button>
              <button onClick={() => open(env)}>Open ↗</button>
              <button onClick={() => installGate(env)} title="Install or update cliogate (required for workspace sync)">
                Install gate
              </button>
            </div>
          </div>
        ))}
      </div>
      {showAdd && (
        <AddEnvironmentDialog onClose={() => setShowAdd(false)} onSubmitted={() => setShowAdd(false)} />
      )}
    </div>
  );
}
