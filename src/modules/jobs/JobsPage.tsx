import { useEffect, useRef, useState } from "react";
import { cancelJob, clearJobHistory, getJobLog, getJobs, JobInfo, onJobLog, onJobUpdate } from "../../lib/ipc";

function duration(job: JobInfo): string {
  const end = job.finishedAt ?? Date.now();
  const s = Math.max(0, Math.round((end - job.startedAt) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

interface FailureSummary {
  title: string;
  general: string;
  technical: string[];
}

function failureSummary(job: JobInfo | undefined, lines: string[]): FailureSummary | null {
  if (!job || job.status !== "failed") return null;

  const schemaConflict = lines.find(
    (line) =>
      line.includes("Unable to install Schema") &&
      line.includes("because the element has been modified locally"),
  );
  if (schemaConflict) {
    const schema = schemaConflict.match(/Schema "([^"]+)"/)?.[1] ?? "a schema";
    return {
      title: "Deployment partially completed",
      general:
        `Creatio installed the package, but it did not replace ${schema}. ` +
        "That item has local changes in the target environment. Preserve or merge those changes, mark the item unchanged, and deploy again.",
      technical: [schemaConflict.trim(), `Process exit code: ${job.exitCode ?? "unavailable"}`],
    };
  }

  const meaningful = lines
    .filter((line) => {
      const text = line.trim();
      if (!text) return false;
      if (/^\[ERR\]\s*-\s*Error\s*$/i.test(text)) return false;
      return (
        text.includes("[ERR]") ||
        /\b(unable to|exception|failed|failure|conflict|unauthorized|forbidden|timed out)\b/i.test(text)
      );
    })
    .slice(-4);

  const installationFinished = lines.some((line) => line.includes("Package installation finished"));
  return {
    title: installationFinished ? "Deployment finished with errors" : "Job failed",
    general: installationFinished
      ? "Creatio reached the end of its installation pipeline, but one or more operations failed. Some package content may already be present, so verify the target before retrying."
      : "The command did not complete successfully. Review the technical details below before retrying.",
    technical: meaningful.length > 0
      ? [...meaningful, `Process exit code: ${job.exitCode ?? "unavailable"}`]
      : [`clio returned exit code ${job.exitCode ?? "unavailable"} without a detailed error message.`],
  };
}

export default function JobsPage() {
  const [jobs, setJobs] = useState<JobInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const logRef = useRef<HTMLPreElement>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selected;

  useEffect(() => {
    getJobs().then((js) => {
      setJobs(js);
      if (js.length > 0 && !selectedRef.current) selectJob(js[0].id);
    });
    const unUpdate = onJobUpdate((job) => {
      setJobs((prev) => {
        const i = prev.findIndex((j) => j.id === job.id);
        if (i === -1) return [job, ...prev];
        const next = [...prev];
        next[i] = job;
        return next;
      });
    });
    const unLog = onJobLog((entry) => {
      if (entry.id === selectedRef.current) {
        setLog((prev) => [...prev, entry.line]);
      }
    });
    return () => {
      unUpdate.then((f) => f());
      unLog.then((f) => f());
    };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  const selectJob = (id: string) => {
    setSelected(id);
    getJobLog(id).then(setLog);
  };

  const selectedJob = jobs.find((job) => job.id === selected);
  const failure = failureSummary(selectedJob, log);

  const cancelSelected = async () => {
    if (!selectedJob) return;
    try {
      await cancelJob(selectedJob.id);
    } catch (e) {
      setLog((previous) => [...previous, `Cannot cancel: ${String(e)}`]);
    }
  };

  return (
    <div className="page-body jobs-layout">
      <div className="page-bar">
        <h1>Jobs</h1>
        {jobs.some((j) => ["succeeded", "failed", "cancelled"].includes(j.status)) && (
          <button
            className="ghost"
            onClick={() =>
              clearJobHistory().then((remaining) => {
                setJobs(remaining);
                if (selected && !remaining.some((j) => j.id === selected)) {
                  setSelected(null);
                  setLog([]);
                }
              })
            }
            title="Remove finished jobs and their logs. Running jobs are kept."
          >
            Clear history
          </button>
        )}
      </div>
      {jobs.length === 0 ? (
        <p className="empty">Nothing has run yet. Actions from other screens appear here with live output.</p>
      ) : (
        <div className="jobs-split">
          <div className="jobs-list">
            {jobs.map((j) => (
              <button
                key={j.id}
                className={`job-row ${selected === j.id ? "sel" : ""}`}
                onClick={() => selectJob(j.id)}
              >
                <span className={`dot ${j.status}`} />
                <span className="job-kind">{j.kind}</span>
                <span className="job-env">{j.env ?? ""}</span>
                <span className="job-time">{duration(j)}</span>
              </button>
            ))}
          </div>
          <div className="job-detail">
            {selected && (
              <>
                <div className="job-detail-bar">
                  <div>
                    <code className="job-cmd">{selectedJob?.displayCommand}</code>
                    <span className="job-phase">Phase: {selectedJob?.phase}</span>
                  </div>
                  {selectedJob && ["queued", "running", "cancelling"].includes(selectedJob.status) && (
                    <button
                      className="danger"
                      disabled={!selectedJob.cancellable || selectedJob.status === "cancelling"}
                      onClick={cancelSelected}
                      title={selectedJob.cancellable
                        ? "Stop this job before it reaches an unsafe phase"
                        : `Cannot stop safely during ${selectedJob.phase}`}
                    >
                      {selectedJob.status === "cancelling" ? "Stopping…" : "Cancel job"}
                    </button>
                  )}
                </div>
                {failure && (
                  <section className="job-failure-summary" aria-label="Failure summary">
                    <div className="job-failure-heading">
                      <span className="job-failure-icon" aria-hidden="true">!</span>
                      <div>
                        <strong>{failure.title}</strong>
                        <span>Action required</span>
                      </div>
                    </div>
                    <div className="job-failure-columns">
                      <div>
                        <h3>In general terms</h3>
                        <p>{selectedJob?.diagnosis?.summary ?? failure.general}</p>
                        {selectedJob?.diagnosis && <p>{selectedJob.diagnosis.cause}</p>}
                      </div>
                      <div>
                        <h3>Technical details</h3>
                        {failure.technical.map((line, index) => (
                          <code key={`${index}-${line}`}>{line}</code>
                        ))}
                      </div>
                    </div>
                    {selectedJob?.diagnosis && (
                      <div className="job-failure-steps">
                        <h3>How to fix it</h3>
                        <ol>
                          {selectedJob.diagnosis.steps.map((step) => <li key={step}>{step}</li>)}
                        </ol>
                      </div>
                    )}
                  </section>
                )}
                <pre className="job-log" ref={logRef}>
                  {log.length > 0 ? log.join("\n") : "— no output yet —"}
                </pre>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
