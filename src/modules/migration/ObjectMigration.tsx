import { useEffect, useState } from "react";
import { ChevronRight, Database, Play, RefreshCw, Search, ShieldAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import ErrorNote from "../../lib/ErrorNote";
import { logError } from "../../lib/errorLog";
import {
  buildObjectMigration,
  EnvSummary,
  listEnvironments,
  listObjects,
  migrateObject,
  ObjectColumn,
  objectColumns,
  ObjectDependency,
  objectDependencies,
  ObjectInfo,
  objectRowCount,
  onJobUpdate,
} from "../../lib/ipc";

const SOURCE = "Migration";

/**
 * Stage 1 of general (non-lookup) data migration: **inspect** an entity object
 * and its foreign-key hierarchy across two environments before any rows move.
 * You pick an object (Lead, Contact, Account, …), see its columns and how many
 * rows each side holds, and expand its dependency tree — the order a later copy
 * has to respect. Read-only: nothing here writes to an environment.
 */
export default function ObjectMigration({ onShowJobs }: { onShowJobs: () => void }) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");

  const [filter, setFilter] = useState("");
  const [objects, setObjects] = useState<ObjectInfo[]>([]);
  const [searching, setSearching] = useState(false);

  const [selected, setSelected] = useState<string | null>(null);
  const [columns, setColumns] = useState<ObjectColumn[]>([]);
  const [deps, setDeps] = useState<ObjectDependency[]>([]);
  const [sourceCount, setSourceCount] = useState<number | null>(null);
  const [targetCount, setTargetCount] = useState<number | null>(null);
  const [targetMissing, setTargetMissing] = useState(false);
  const [loadingDetail, setLoadingDetail] = useState(false);

  const [remapOwner, setRemapOwner] = useState(true);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [skipBackup, setSkipBackup] = useState(false);

  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const fail = (e: unknown, context: string, forEnv?: string) => {
    const message = String(e);
    setError(message);
    logError(SOURCE, message, { context, env: forEnv });
  };

  useEffect(() => {
    listEnvironments()
      .then((list) => {
        setEnvs(list);
        setSource(list.find((item) => item.isActive)?.name ?? list[0]?.name ?? "");
        setTarget(list.find((item) => !item.isActive)?.name ?? "");
      })
      .catch((e) => fail(e, "Load environments"));
  }, []);

  // Surface the outcome of an object migration the same way the lookup screen
  // does — the detail lives in Jobs.
  useEffect(() => {
    const un = onJobUpdate((job) => {
      if (job.kind === "migrate-object" && ["succeeded", "failed"].includes(job.status)) {
        setNotice(
          job.status === "succeeded"
            ? `Migration to ${job.env} finished — see Jobs for the row counts.`
            : `Migration to ${job.env} failed — open Jobs for the reason and the rollback file path.`,
        );
      }
    });
    return () => { un.then((fn) => fn()); };
  }, []);

  const runSearch = async (term = filter) => {
    if (!source) return;
    setSearching(true);
    setError("");
    try {
      setObjects(await listObjects(source, term.trim()));
    } catch (e) {
      setObjects([]);
      fail(e, "Search objects", source);
    } finally {
      setSearching(false);
    }
  };

  // A picked object's columns + dependencies come from the source; the row
  // counts come from both sides so you can see what a copy would change.
  const loadDetail = async (table: string) => {
    setSelected(table);
    setLoadingDetail(true);
    setError("");
    setNotice("");
    setPreview(null);
    setColumns([]);
    setDeps([]);
    setSourceCount(null);
    setTargetCount(null);
    setTargetMissing(false);
    try {
      const [cols, dependencies, srcCount] = await Promise.all([
        objectColumns(source, table),
        objectDependencies(source, table),
        objectRowCount(source, table),
      ]);
      setColumns(cols);
      setDeps(dependencies);
      setSourceCount(srcCount);
    } catch (e) {
      fail(e, "Read object", source);
    } finally {
      setLoadingDetail(false);
    }
    // Target may not have the table at all — treat a failure there as "missing"
    // rather than a hard error, since that is itself useful information.
    if (target) {
      try {
        setTargetCount(await objectRowCount(target, table));
      } catch {
        setTargetMissing(true);
      }
    }
  };

  const refreshCounts = async () => {
    if (!selected) return;
    if (source) {
      try {
        setSourceCount(await objectRowCount(source, selected));
      } catch (e) {
        fail(e, "Refresh source count", source);
      }
    }
    if (target) {
      setTargetMissing(false);
      try {
        setTargetCount(await objectRowCount(target, selected));
      } catch {
        setTargetMissing(true);
      }
    }
  };

  const runPreview = async () => {
    if (!selected || !source || !target) return;
    setPreviewBusy(true);
    setError("");
    setNotice("");
    try {
      setPreview(await buildObjectMigration({ sourceEnv: source, targetEnv: target, table: selected, remapOwner }));
    } catch (e) {
      setPreview(null);
      fail(e, "Preview object SQL", source);
    } finally {
      setPreviewBusy(false);
    }
  };

  const openConfirm = () => {
    setConfirmText("");
    setSkipBackup(false);
    setNotice("");
    setError("");
    setConfirmOpen(true);
  };

  const migrate = async () => {
    if (!selected) return;
    try {
      await migrateObject({ sourceEnv: source, targetEnv: target, table: selected, remapOwner, skipBackup });
      setConfirmOpen(false);
      setNotice(`Migrating ${selected} to ${target} — follow it in Jobs.`);
    } catch (e) {
      fail(e, "Migrate object", target);
    }
  };

  const referenceColumns = new Set(deps.map((dep) => dep.column));
  const ownerColumns = deps.filter((dep) => dep.referencesTable === "SysAdminUnit").map((dep) => dep.column);
  const sameEnv = !!source && source === target;

  return (
    <div className="grid gap-4">
      <p className="text-sm text-muted-foreground">
        Inspect and copy a single object's rows between environments. Pick an object, compare how
        many rows each side holds, expand its foreign-key hierarchy, then copy its rows as a
        full-column upsert. One object at a time — related objects it depends on are not pulled
        along, so make sure the target already has them.
      </p>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid gap-2 rounded-lg border p-3">
          <Label htmlFor="obj-source">Source (read from)</Label>
          <Select value={source} onValueChange={(v) => { setSource(v); setObjects([]); setSelected(null); }}>
            <SelectTrigger id="obj-source" className="w-full">
              <SelectValue placeholder="Select an environment" />
            </SelectTrigger>
            <SelectContent>
              {envs.map((item) => (
                <SelectItem key={item.name} value={item.name}>{item.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-2 rounded-lg border p-3">
          <Label htmlFor="obj-target">Target (compare to)</Label>
          <Select value={target} onValueChange={setTarget}>
            <SelectTrigger id="obj-target" className="w-full">
              <SelectValue placeholder="Select an environment" />
            </SelectTrigger>
            <SelectContent>
              {envs.map((item) => (
                <SelectItem key={item.name} value={item.name}>{item.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {sameEnv && <p className="text-sm text-destructive">Choose two different environments.</p>}

      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => { e.preventDefault(); runSearch(); }}
      >
        <div className="relative min-w-64 flex-1">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" aria-hidden="true" />
          <Input
            className="pl-8"
            placeholder="Search objects — e.g. Lead, Contact, Account"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <Button type="submit" variant="outline" disabled={!source || searching}>
          {searching ? "Searching…" : "Search"}
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={!source || searching}
          onClick={() => runSearch()}
          title="Refresh the object list from the source"
        >
          <RefreshCw aria-hidden="true" />
          Refresh
        </Button>
      </form>

      {error && <ErrorNote error={error} />}

      <div className="grid gap-4 lg:grid-cols-[minmax(0,20rem)_1fr]">
        {/* Object picker */}
        <div className="grid content-start gap-2 rounded-lg border">
          <div className="border-b p-3 text-sm text-muted-foreground">
            {searching
              ? "Searching…"
              : objects.length === 0
                ? "No objects — search above."
                : `${objects.length} object${objects.length === 1 ? "" : "s"}${objects.length === 300 ? " (showing first 300)" : ""}`}
          </div>
          <ScrollArea className="h-96">
            <ul className="divide-y">
              {objects.map((obj) => (
                <li key={obj.table}>
                  <button
                    className={cn(
                      "flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:bg-muted/40",
                      selected === obj.table && "bg-accent/10",
                    )}
                    onClick={() => loadDetail(obj.table)}
                  >
                    <span className="flex items-center gap-2">
                      <Database className="size-3.5 text-muted-foreground" aria-hidden="true" />
                      <span className="text-sm">{obj.table}</span>
                    </span>
                    {obj.package && (
                      <Badge variant="secondary" className="shrink-0 text-[10px]">{obj.package}</Badge>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </ScrollArea>
        </div>

        {/* Detail: counts, columns, hierarchy */}
        <div className="grid content-start gap-4">
          {!selected ? (
            <div className="grid h-full min-h-48 place-items-center rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
              Select an object to see its row counts, columns, and dependency hierarchy.
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className="flex items-center gap-2 text-lg font-semibold">
                  <Database className="size-4" aria-hidden="true" />
                  {selected}
                </h2>
                <Button size="sm" variant="outline" onClick={refreshCounts} disabled={loadingDetail}>
                  <RefreshCw aria-hidden="true" />
                  Refresh counts
                </Button>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <div className="rounded-lg border p-3">
                  <div className="text-xs text-muted-foreground">{source || "source"}</div>
                  <div className="text-2xl font-semibold tabular-nums">
                    {sourceCount === null ? "…" : sourceCount.toLocaleString()}
                  </div>
                  <div className="text-xs text-muted-foreground">rows</div>
                </div>
                <div className="rounded-lg border p-3">
                  <div className="text-xs text-muted-foreground">{target || "target"}</div>
                  <div className="text-2xl font-semibold tabular-nums">
                    {targetMissing ? "—" : targetCount === null ? "…" : targetCount.toLocaleString()}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {targetMissing ? "table not found on target" : "rows"}
                  </div>
                </div>
              </div>

              <section className="grid gap-3 rounded-lg border p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold">Copy rows to {target || "target"}</h3>
                  <Button variant="ghost" size="sm" onClick={onShowJobs}>Jobs</Button>
                </div>
                <p className="flex items-start gap-2 text-xs text-muted-foreground">
                  <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <span>
                    Full-column upsert keyed on <code className="rounded bg-muted px-1 font-mono">Id</code>,
                    written as raw SQL — it bypasses Creatio's validation and business logic. Unless turned
                    off, a runnable rollback script is written first (its path is logged in the job).
                  </span>
                </p>

                <Label className="flex items-start gap-2 font-normal">
                  <Checkbox
                    className="mt-0.5"
                    checked={remapOwner}
                    onCheckedChange={(checked) => setRemapOwner(!!checked)}
                  />
                  <span>
                    Remap ownership to the target's Supervisor
                    {ownerColumns.length > 0 && (
                      <span className="ml-1 text-xs text-muted-foreground">({ownerColumns.join(", ")})</span>
                    )}
                    <span className="block text-xs text-muted-foreground">
                      Owner/created-by columns point at users that differ per environment; remapping avoids
                      broken references.
                    </span>
                  </span>
                </Label>

                <div className="flex flex-wrap items-center gap-2">
                  <Button variant="outline" disabled={!target || sameEnv || previewBusy} onClick={runPreview}>
                    {previewBusy ? "Building…" : "Preview SQL"}
                  </Button>
                  <Button variant="destructive" disabled={!target || sameEnv} onClick={openConfirm}>
                    <Play aria-hidden="true" />
                    Migrate {selected} to {target || "target"}…
                  </Button>
                </div>

                {notice && <p className="text-sm text-muted-foreground">{notice}</p>}

                {preview !== null && (
                  <div className="grid gap-1">
                    <Label className="text-xs">Dry run — the SQL that would run on {target}</Label>
                    <ScrollArea className="h-56 rounded-lg border">
                      <pre className="p-3 font-mono text-xs whitespace-pre-wrap break-all">{preview}</pre>
                    </ScrollArea>
                  </div>
                )}
              </section>

              <section className="grid gap-2">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold">Dependency hierarchy</h3>
                  <Badge variant="secondary">{deps.length} direct</Badge>
                </div>
                <p className="text-xs text-muted-foreground">
                  Foreign keys this object needs. Expand a row to walk further down; a repeated table
                  in the same path (a cycle, e.g. Account ↔ Contact) is marked and not re-expanded.
                </p>
                <div className="rounded-lg border p-2">
                  {loadingDetail ? (
                    <p className="p-2 text-sm text-muted-foreground">Reading dependencies…</p>
                  ) : (
                    <DependencyTree
                      env={source}
                      table={selected}
                      deps={deps}
                      path={[selected]}
                      onError={(e) => fail(e, "Read dependencies", source)}
                    />
                  )}
                </div>
              </section>

              <section className="grid gap-2">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold">Columns</h3>
                  <Badge variant="secondary">{columns.length}</Badge>
                </div>
                <ScrollArea className="h-56 rounded-lg border">
                  <ul className="divide-y text-sm">
                    {columns.map((col) => (
                      <li key={col.name} className="flex items-center justify-between gap-2 px-3 py-1.5">
                        <span className="flex items-center gap-2">
                          <span className="font-mono">{col.name}</span>
                          {referenceColumns.has(col.name) && (
                            <Badge className="border-transparent bg-accent/15 text-[10px] text-accent-foreground">
                              ref
                            </Badge>
                          )}
                        </span>
                        <span className="flex items-center gap-2 text-xs text-muted-foreground">
                          <span>{col.dataType}</span>
                          {!col.nullable && <span className="text-warning">NOT NULL</span>}
                        </span>
                      </li>
                    ))}
                  </ul>
                </ScrollArea>
              </section>
            </>
          )}
        </div>
      </div>

      <Dialog open={confirmOpen} onOpenChange={(open) => !open && setConfirmOpen(false)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Copy {selected} to {target}?</DialogTitle>
            <DialogDescription>
              Writes {sourceCount !== null ? sourceCount.toLocaleString() : "the source's"} row(s) of{" "}
              <strong>{selected}</strong> from <strong>{source}</strong> into <strong>{target}</strong> as a
              full-column upsert keyed on Id. Existing rows with the same Id are overwritten; it does not
              copy related objects, and it bypasses Creatio validation.
            </DialogDescription>
          </DialogHeader>
          <Label className="flex items-center gap-2 font-normal">
            <Checkbox checked={!skipBackup} onCheckedChange={(checked) => setSkipBackup(!checked)} />
            Write a rollback script first (recommended)
          </Label>
          <div className="grid gap-2">
            <Label htmlFor="obj-confirm">
              Type <strong>{target || "the target environment"}</strong> to confirm
            </Label>
            <Input
              id="obj-confirm"
              value={confirmText}
              onChange={(event) => setConfirmText(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmOpen(false)}>Cancel</Button>
            <Button variant="destructive" disabled={!target || confirmText !== target} onClick={migrate}>
              Copy rows
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/**
 * A lazily-expanding foreign-key tree. Each referenced table can be opened to
 * fetch its own dependencies. A table already on the current path is a cycle and
 * is shown but not expandable, so Account ↔ Contact can't recurse forever.
 */
function DependencyTree({
  env,
  table,
  deps,
  path,
  onError,
}: {
  env: string;
  table: string;
  deps: ObjectDependency[];
  path: string[];
  onError: (e: unknown) => void;
}) {
  if (deps.length === 0) {
    return <p className="px-2 py-1 text-xs text-muted-foreground">No foreign keys — {table} is a leaf.</p>;
  }
  return (
    <ul className="grid gap-0.5">
      {deps.map((dep) => (
        <DependencyNode
          key={`${dep.column}:${dep.referencesTable}`}
          env={env}
          dep={dep}
          path={path}
          onError={onError}
        />
      ))}
    </ul>
  );
}

function DependencyNode({
  env,
  dep,
  path,
  onError,
}: {
  env: string;
  dep: ObjectDependency;
  path: string[];
  onError: (e: unknown) => void;
}) {
  const [open, setOpen] = useState(false);
  const [childDeps, setChildDeps] = useState<ObjectDependency[] | null>(null);
  const [loading, setLoading] = useState(false);
  const isCycle = path.includes(dep.referencesTable);

  const toggle = async () => {
    if (isCycle) return;
    const next = !open;
    setOpen(next);
    if (next && childDeps === null) {
      setLoading(true);
      try {
        setChildDeps(await objectDependencies(env, dep.referencesTable));
      } catch (e) {
        onError(e);
        setChildDeps([]);
      } finally {
        setLoading(false);
      }
    }
  };

  return (
    <li>
      <div className="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-muted/40">
        <button
          className={cn("flex items-center gap-1 text-left", isCycle && "cursor-default")}
          onClick={toggle}
          disabled={isCycle}
        >
          <ChevronRight
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-90",
              isCycle && "opacity-0",
            )}
            aria-hidden="true"
          />
          <span className="font-mono text-xs text-muted-foreground">{dep.column}</span>
          <span className="text-xs text-muted-foreground">→</span>
          <span className="text-sm">{dep.referencesTable}</span>
        </button>
        {isCycle && (
          <Badge className="border-transparent bg-warning/15 text-[10px] text-warning">cycle</Badge>
        )}
      </div>
      {open && !isCycle && (
        <div className="ml-4 border-l pl-2">
          {loading ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">Reading…</p>
          ) : (
            <DependencyTree
              env={env}
              table={dep.referencesTable}
              deps={childDeps ?? []}
              path={[...path, dep.referencesTable]}
              onError={onError}
            />
          )}
        </div>
      )}
    </li>
  );
}
