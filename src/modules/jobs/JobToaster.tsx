import { useEffect, useState } from "react";
import { getJobs, JobInfo, onJobUpdate } from "../../lib/ipc";

interface Toast {
  id: string;
  label: string;
  env: string | null;
  phase: string;
  state: "running" | "succeeded" | "failed" | "cancelled";
}

const ACTIVE = new Set(["queued", "running", "cancelling"]);
const DISMISS_MS = 6000;

/**
 * Global, always-mounted job indicator in the top-right corner. Subscribes once
 * to job-update events (and seeds from getJobs on mount so already-running jobs
 * show immediately). Running jobs stay pinned; finished jobs flash their outcome
 * and auto-dismiss. Clicking any toast opens the Jobs screen.
 */
export default function JobToaster({ onShowJobs }: { onShowJobs: () => void }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  useEffect(() => {
    const timers = new Map<string, number>();

    const apply = (job: JobInfo) => {
      const running = ACTIVE.has(job.status);
      const toast: Toast = {
        id: job.id,
        label: job.displayCommand || job.kind,
        env: job.env,
        phase: job.phase,
        state: running ? "running" : (job.status as Toast["state"]),
      };
      setToasts((prev) => {
        const next = prev.filter((t) => t.id !== job.id);
        // Only surface running jobs and terminal outcomes; ignore queued noise.
        if (running || job.status === "succeeded" || job.status === "failed" || job.status === "cancelled") {
          next.push(toast);
        }
        return next;
      });
      const existing = timers.get(job.id);
      if (existing) {
        window.clearTimeout(existing);
        timers.delete(job.id);
      }
      if (!running) {
        const handle = window.setTimeout(() => {
          setToasts((prev) => prev.filter((t) => t.id !== job.id));
          timers.delete(job.id);
        }, DISMISS_MS);
        timers.set(job.id, handle);
      }
    };

    getJobs()
      .then((jobs) => jobs.filter((j) => ACTIVE.has(j.status)).forEach(apply))
      .catch(() => {});
    const un = onJobUpdate(apply);

    return () => {
      un.then((f) => f());
      timers.forEach((h) => window.clearTimeout(h));
    };
  }, []);

  if (toasts.length === 0) return null;

  const icon = (s: Toast["state"]) =>
    s === "running" ? "⏳" : s === "succeeded" ? "✅" : s === "cancelled" ? "⊘" : "✗";

  return (
    <div className="job-toaster">
      {toasts.map((t) => (
        <button
          key={t.id}
          className={`job-toast ${t.state}`}
          onClick={onShowJobs}
          title="Open Jobs"
        >
          <span className="job-toast-icon">{icon(t.state)}</span>
          <span className="job-toast-body">
            <span className="job-toast-title">{t.label}</span>
            <span className="job-toast-sub">
              {t.state === "running"
                ? t.phase || "working…"
                : t.state === "succeeded"
                  ? "done"
                  : t.state === "cancelled"
                    ? "cancelled"
                    : "failed"}
              {t.env ? ` · ${t.env}` : ""}
            </span>
          </span>
        </button>
      ))}
    </div>
  );
}
