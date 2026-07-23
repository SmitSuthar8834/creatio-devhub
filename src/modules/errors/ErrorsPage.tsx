import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import ErrorLogPanel from "../../lib/ErrorLogPanel";
import { useErrorLog } from "../../lib/errorLog";

/**
 * App-wide error history. Every screen records its failures into the shared
 * log (see lib/errorLog); this page shows them all, filterable by source.
 */
export default function ErrorsPage() {
  const all = useErrorLog();
  const [source, setSource] = useState<string | null>(null);

  // Distinct sources with a live count, so the filter reflects what's there.
  const sources = useMemo(() => {
    const counts = new Map<string, number>();
    for (const entry of all) counts.set(entry.source, (counts.get(entry.source) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [all]);

  // A source that no longer has entries shouldn't stay selected.
  const activeSource = source && sources.some(([name]) => name === source) ? source : null;

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Errors</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Failures collected from across DevHub, most recent first. Each shows its cause and how to
          fix it. Stored on this device.
        </p>
      </div>

      {sources.length > 0 && (
        <div className="flex flex-wrap gap-2">
          <FilterChip active={activeSource === null} onClick={() => setSource(null)}>
            All <span className="opacity-60">{all.length}</span>
          </FilterChip>
          {sources.map(([name, count]) => (
            <FilterChip key={name} active={activeSource === name} onClick={() => setSource(name)}>
              {name} <span className="opacity-60">{count}</span>
            </FilterChip>
          ))}
        </div>
      )}

      <ErrorLogPanel source={activeSource ?? undefined} showSource />
    </div>
  );
}

function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Button
      variant={active ? "default" : "outline"}
      size="sm"
      onClick={onClick}
      className={cn("gap-1.5", !active && "text-muted-foreground")}
    >
      {children}
    </Button>
  );
}
