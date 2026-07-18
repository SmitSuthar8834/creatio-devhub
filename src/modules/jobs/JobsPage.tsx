import { useEffect, useRef, useState } from "react";
import { cancelJob, getJobLog, getJobs, JobInfo, onJobLog, onJobUpdate } from "../../lib/ipc";

function duration(job: JobInfo): string {
  const end = job.finishedAt ?? Date.now();
  const s = Math.max(0, Math.round((end - job.startedAt) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
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
