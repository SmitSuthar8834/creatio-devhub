import { CircleCheck, TriangleAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import ErrorNote from "./ErrorNote";
import { AppErrorEntry, clearErrors, dismissError, useErrorLog } from "./errorLog";

interface ErrorLogPanelProps {
  /** Show only this source's errors. Omit for the whole app. */
  source?: string;
  /** When set, the panel groups the "Source" badge in (used by the global view). */
  showSource?: boolean;
  /** Optional per-entry action, e.g. reload a failed SQL query into the editor. */
  onReuse?: (entry: AppErrorEntry) => void;
  reuseLabel?: string;
  /** Copy shown when there are no errors. */
  emptyHint?: string;
}

/**
 * Renders the shared error log (optionally scoped to one source) with dismiss,
 * clear-all, and an optional per-entry reuse action. Used by both the global
 * Errors page and SQL's Errors tab so they stay identical.
 */
export default function ErrorLogPanel({
  source,
  showSource = false,
  onReuse,
  reuseLabel = "Reuse",
  emptyHint = "When a run or action fails, it will be collected here.",
}: ErrorLogPanelProps) {
  const entries = useErrorLog(source);

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          {entries.length === 0
            ? "No errors logged."
            : `${entries.length} error${entries.length === 1 ? "" : "s"}, most recent first. Stored on this device.`}
        </p>
        {entries.length > 0 && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              if (window.confirm(`Clear ${entries.length} logged error${entries.length === 1 ? "" : "s"}?`)) {
                clearErrors(source);
              }
            }}
          >
            Clear all
          </Button>
        )}
      </div>

      {entries.length === 0 ? (
        <div className="grid justify-items-center gap-1 rounded-lg border border-dashed p-8 text-center">
          <CircleCheck className="size-6 text-success" aria-hidden="true" />
          <p className="text-sm font-medium">All clear</p>
          <p className="text-sm text-muted-foreground">{emptyHint}</p>
        </div>
      ) : (
        <div className="grid gap-3">
          {entries.map((entry) => (
            <article key={entry.id} className="grid gap-2 rounded-lg border p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <TriangleAlert className="size-4 text-destructive" aria-hidden="true" />
                  {showSource && <Badge variant="secondary">{entry.source}</Badge>}
                  {entry.context && <Badge variant="outline">{entry.context}</Badge>}
                  <span className="text-xs text-muted-foreground">
                    {entry.env ? `${entry.env} · ` : ""}
                    {new Date(entry.at).toLocaleString()}
                  </span>
                </div>
                <div className="flex flex-wrap gap-2">
                  {onReuse && (
                    <Button size="sm" variant="outline" onClick={() => onReuse(entry)}>
                      {reuseLabel}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-destructive hover:text-destructive"
                    onClick={() => dismissError(entry.id)}
                  >
                    Dismiss
                  </Button>
                </div>
              </div>
              <ErrorNote error={entry.message} />
              {entry.detail && (
                <code className="truncate font-mono text-xs text-muted-foreground">
                  {entry.detail.replace(/\s+/g, " ").slice(0, 140)}
                </code>
              )}
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
