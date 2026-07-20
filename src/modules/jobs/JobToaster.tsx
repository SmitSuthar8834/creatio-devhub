import { useEffect } from "react";
import { toast } from "sonner";
import { getJobs, JobInfo, onJobUpdate } from "../../lib/ipc";

const ACTIVE = new Set(["queued", "running", "cancelling"]);
const DISMISS_MS = 6000;

/**
 * Headless driver for job notifications, rendered through the shared sonner
 * <Toaster>. One toast per job, keyed by job id: it shows as a loading toast
 * while the job runs (phase as the description), then flips to the outcome and
 * auto-dismisses. Quiet jobs (background health checks and similar) are skipped
 * entirely — their feedback lives in the UI that started them.
 */
export default function JobToaster({ onShowJobs }: { onShowJobs: () => void }) {
  useEffect(() => {
    const apply = (job: JobInfo) => {
      if (job.quiet) return;
      const label = job.displayCommand || job.kind;
      const env = job.env ? ` · ${job.env}` : "";
      const action = { label: "Open Jobs", onClick: onShowJobs };

      if (ACTIVE.has(job.status)) {
        toast.loading(label, { id: job.id, description: (job.phase || "working…") + env, action });
      } else if (job.status === "succeeded") {
        toast.success(label, { id: job.id, description: `done${env}`, action, duration: DISMISS_MS });
      } else if (job.status === "cancelled") {
        toast.info(label, { id: job.id, description: `cancelled${env}`, action, duration: DISMISS_MS });
      } else if (job.status === "failed") {
        toast.error(label, {
          id: job.id,
          description: (job.diagnosis?.summary ?? "failed") + env,
          action,
          duration: DISMISS_MS,
        });
      }
    };

    // Seed with jobs already running so a mid-job app reload still shows them.
    getJobs()
      .then((jobs) => jobs.filter((j) => !j.quiet && ACTIVE.has(j.status)).forEach(apply))
      .catch(() => {});
    const un = onJobUpdate(apply);
    return () => {
      un.then((f) => f());
    };
  }, [onShowJobs]);

  return null;
}
