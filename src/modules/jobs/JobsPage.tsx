import { useEffect, useRef, useState } from "react";
import { CircleAlert, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
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
        /\b(unable to|exception|failed|failure|conflict|unauthorized|forbidden|timed out|internal server error)\b/i.test(text)
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

const DOT: Record<string, string> = {
  queued: "bg-muted-foreground/50",
  running: "bg-primary animate-pulse",
  cancelling: "bg-warning animate-pulse",
  cancelled: "bg-muted-foreground/50",
  succeeded: "bg-success",
  failed: "bg-destructive",
};

export default function JobsPage() {
  const [jobs, setJobs] = useState<JobInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [showList, setShowList] = useState(true);
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
    <div className="flex h-full flex-col p-6">
      <div className="mb-5 flex items-center justify-between gap-3">
        <div className="flex items-center gap-1.5">
          {jobs.length > 0 && (
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setShowList((v) => !v)}
              title={showList ? "Hide the job list" : "Show the job list"}
              aria-label={showList ? "Hide the job list" : "Show the job list"}
            >
              {showList ? <PanelLeftClose aria-hidden="true" /> : <PanelLeftOpen aria-hidden="true" />}
            </Button>
          )}
          <h1 className="text-xl font-semibold tracking-tight">Jobs</h1>
        </div>
        {jobs.some((j) => ["succeeded", "failed", "cancelled"].includes(j.status)) && (
          <Button
            variant="outline"
            size="sm"
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
          </Button>
        )}
      </div>

      {jobs.length === 0 ? (
        <p className="text-muted-foreground">
          Nothing has run yet. Actions from other screens appear here with live output.
        </p>
      ) : (
        <div
          className={cn(
            "grid min-h-0 flex-1 gap-4",
            showList ? "grid-cols-[280px_minmax(0,1fr)]" : "grid-cols-1",
          )}
        >
          <div className={cn("flex min-h-0 flex-col gap-1.5 overflow-y-auto pr-1", !showList && "hidden")}>
            {jobs.map((j) => (
              <button
                key={j.id}
                onClick={() => selectJob(j.id)}
                className={cn(
                  "grid grid-cols-[8px_1fr_auto] items-center gap-2 rounded-lg border bg-card px-3 py-2 text-left text-sm transition-colors hover:bg-accent/10",
                  selected === j.id && "border-primary ring-1 ring-primary",
                )}
              >
                <span className={cn("size-2 rounded-full", DOT[j.status] ?? "bg-muted-foreground/50")} />
                <span className="min-w-0">
                  <span className="block truncate font-medium">{j.kind}</span>
                  <span className="block truncate text-xs text-muted-foreground">{j.env ?? ""}</span>
                </span>
                <span className="font-mono text-xs tabular-nums text-muted-foreground">{duration(j)}</span>
              </button>
            ))}
          </div>

          <div className="flex min-h-0 min-w-0 flex-col gap-3">
            {selected && (
              <>
                <div className="flex items-start justify-between gap-3 rounded-lg border bg-card px-4 py-3">
                  <div className="min-w-0">
                    <code className="block truncate font-mono text-xs">{selectedJob?.displayCommand}</code>
                    <span className="text-xs text-muted-foreground">Phase: {selectedJob?.phase}</span>
                  </div>
                  {selectedJob && ["queued", "running", "cancelling"].includes(selectedJob.status) && (
                    <Button
                      variant="destructive"
                      size="sm"
                      disabled={!selectedJob.cancellable || selectedJob.status === "cancelling"}
                      onClick={cancelSelected}
                      title={selectedJob.cancellable
                        ? "Stop this job before it reaches an unsafe phase"
                        : `Cannot stop safely during ${selectedJob.phase}`}
                    >
                      {selectedJob.status === "cancelling" ? "Stopping…" : "Cancel job"}
                    </Button>
                  )}
                </div>

                {failure && (
                  <section
                    className="rounded-lg border border-destructive/40 bg-destructive/5 p-4"
                    aria-label="Failure summary"
                  >
                    <div className="mb-3 flex items-center gap-2">
                      <CircleAlert className="size-4 text-destructive" aria-hidden="true" />
                      <div className="flex items-baseline gap-2">
                        <strong className="text-sm">{selectedJob?.diagnosis?.summary ?? failure.title}</strong>
                        <Badge variant="destructive" className="text-[10px]">Action required</Badge>
                      </div>
                    </div>
                    <div className="grid gap-4 lg:grid-cols-2">
                      <div className="text-sm">
                        <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                          In general terms
                        </h3>
                        <p>{selectedJob?.diagnosis?.cause ?? failure.general}</p>
                      </div>
                      <div className="min-w-0 text-sm">
                        <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                          Technical details
                        </h3>
                        {failure.technical.map((line, index) => (
                          <code
                            key={`${index}-${line}`}
                            className="mb-1 block truncate rounded bg-muted px-2 py-1 font-mono text-xs"
                            title={line}
                          >
                            {line}
                          </code>
                        ))}
                      </div>
                    </div>
                    {selectedJob?.diagnosis && (
                      <div className="mt-3 text-sm">
                        <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                          How to fix it
                        </h3>
                        <ol className="list-decimal space-y-1 pl-5">
                          {selectedJob.diagnosis.steps.map((step) => <li key={step}>{step}</li>)}
                        </ol>
                      </div>
                    )}
                  </section>
                )}

                <pre
                  ref={logRef}
                  className="min-h-0 flex-1 overflow-auto rounded-lg bg-[#0F0F1A] p-4 font-mono text-xs leading-relaxed text-[#E5E5EA] dark:bg-muted"
                >
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
