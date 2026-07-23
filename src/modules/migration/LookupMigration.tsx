import { useEffect, useMemo, useState } from "react";
import { ShieldAlert } from "lucide-react";
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
import ErrorNote from "../../lib/ErrorNote";
import { logError } from "../../lib/errorLog";
import {
  buildLookupMigration, EnvSummary, listEnvironments, listLookups, LookupInfo,
  migrateLookups, onJobUpdate,
} from "../../lib/ipc";

const SOURCE = "Migration";

/** Write-side surface for reference-data migration: pick lookups in a source
 *  environment, preview the idempotent upsert, then apply it to a target as a
 *  mutating, env-locked job. Read the Compare → Lookups tab first to see what
 *  actually differs; this screen is the action, not the diff. */
export default function MigrationPage({ onShowJobs }: { onShowJobs: () => void }) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [lookups, setLookups] = useState<LookupInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<string | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [skipBackup, setSkipBackup] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  // Show the failure inline and record it into the app-wide Errors view.
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

  // Loading the source's lookups resets everything downstream of it.
  useEffect(() => {
    if (!source) return;
    setLoading(true);
    setError("");
    setSelected(new Set());
    setPreview(null);
    listLookups(source)
      .then(setLookups)
      .catch((e) => { setLookups([]); fail(e, "Load lookups", source); })
      .finally(() => setLoading(false));
  }, [source]);

  useEffect(() => {
    const un = onJobUpdate((job) => {
      if (job.kind === "migrate-lookups" && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        if (job.status === "succeeded") setNotice(`Migration to ${job.env} finished — check Jobs for the row counts.`);
        else if (job.status === "failed") setNotice(`Migration to ${job.env} failed — open Jobs for the reason and the rollback file path.`);
      }
    });
    return () => { un.then((fn) => fn()); };
  }, []);

  const visible = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return lookups;
    return lookups.filter(
      (item) =>
        item.name.toLowerCase().includes(needle) ||
        item.package.toLowerCase().includes(needle) ||
        item.table.toLowerCase().includes(needle),
    );
  }, [lookups, filter]);

  const toggle = (table: string) => {
    const next = new Set(selected);
    if (next.has(table)) next.delete(table); else next.add(table);
    setSelected(next);
    setPreview(null);
  };

  const selectAllVisible = () => {
    const next = new Set(selected);
    visible.forEach((item) => next.add(item.table));
    setSelected(next);
    setPreview(null);
  };

  const clearSelection = () => {
    setSelected(new Set());
    setPreview(null);
  };

  const runPreview = async () => {
    setPreviewBusy(true);
    setError("");
    try {
      setPreview(await buildLookupMigration(source, [...selected]));
    } catch (e) {
      setPreview(null);
      fail(e, "Preview SQL", source);
    } finally {
      setPreviewBusy(false);
    }
  };

  const openConfirm = () => {
    setConfirmText("");
    setSkipBackup(false);
    setNotice("");
    setConfirmOpen(true);
  };

  const migrate = async () => {
    try {
      await migrateLookups({ sourceEnv: source, targetEnv: target, tables: [...selected], skipBackup });
      setConfirmOpen(false);
      setNotice(`Migrating ${selected.size} lookup(s) to ${target} — follow it in Jobs.`);
    } catch (e) {
      fail(e, "Migrate", target);
    }
  };

  const sameEnv = !!source && source === target;

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-semibold">Reference data (lookups)</h2>
        <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
      </div>

      <p className="text-sm text-muted-foreground">
        Copies lookup (reference-data) values from a source environment to a target — the data
        package deployment does not carry unless it is bound to a package. Values are matched on
        their Id, so migrating updates existing rows and adds missing ones without breaking the
        references other records rely on. To see what differs first, use Compare → Lookups.
      </p>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid gap-2 rounded-lg border p-3">
          <Label htmlFor="mig-source">Source (read from)</Label>
          <Select value={source} onValueChange={setSource}>
            <SelectTrigger id="mig-source" className="w-full">
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
          <Label htmlFor="mig-target">Target (write to)</Label>
          <Select value={target} onValueChange={setTarget}>
            <SelectTrigger id="mig-target" className="w-full">
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

      {sameEnv && (
        <p className="text-sm text-destructive">Choose two different environments.</p>
      )}

      <p className="flex items-start gap-2 rounded-lg border border-dashed p-3 text-xs text-muted-foreground">
        <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <span>
          Migrating writes to the target environment. Unless you turn it off, DevHub first records a
          runnable rollback script (its path is logged in the job) so the change can be undone.
        </span>
      </p>

      {error && <ErrorNote error={error} />}
      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}

      <div className="grid gap-2 rounded-lg border">
        <div className="flex flex-wrap items-center gap-2 border-b p-3">
          <Input
            placeholder="Filter by name, package, or table…"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            className="max-w-xs"
          />
          <span className="text-sm text-muted-foreground">
            {loading ? "Loading lookups…" : `${visible.length} lookups`}
            {selected.size > 0 ? ` · ${selected.size} selected` : ""}
          </span>
          <div className="ml-auto flex gap-2">
            <Button size="sm" variant="outline" disabled={visible.length === 0} onClick={selectAllVisible}>
              Select all shown
            </Button>
            <Button size="sm" variant="ghost" disabled={selected.size === 0} onClick={clearSelection}>
              Clear
            </Button>
          </div>
        </div>

        {loading ? (
          <p className="p-4 text-muted-foreground">Reading the lookup registry from {source}…</p>
        ) : visible.length === 0 ? (
          <p className="p-4 text-muted-foreground">
            {lookups.length === 0
              ? "No lookups loaded. The source environment needs the cliogate helper to read them."
              : "No lookups match the filter."}
          </p>
        ) : (
          <ScrollArea className="h-72">
            <ul className="divide-y">
              {visible.map((item) => (
                <li key={item.table}>
                  <Label className="flex cursor-pointer items-center gap-3 px-3 py-2 font-normal hover:bg-muted/40">
                    <Checkbox
                      checked={selected.has(item.table)}
                      onCheckedChange={() => toggle(item.table)}
                    />
                    <span className="flex-1">
                      <span className="text-sm">{item.name}</span>
                      <span className="ml-2 font-mono text-xs text-muted-foreground">{item.table}</span>
                    </span>
                    {item.package && (
                      <Badge variant="secondary" className="shrink-0">{item.package}</Badge>
                    )}
                  </Label>
                </li>
              ))}
            </ul>
          </ScrollArea>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" disabled={selected.size === 0 || previewBusy} onClick={runPreview}>
          {previewBusy ? "Building…" : "Preview SQL"}
        </Button>
        <Button
          variant="destructive"
          disabled={selected.size === 0 || sameEnv || !target}
          onClick={openConfirm}
        >
          Migrate {selected.size || ""} to {target || "target"}…
        </Button>
      </div>

      {preview !== null && (
        <div className="grid gap-2">
          <Label>Dry run — the SQL that would run on {target || "the target"}</Label>
          <ScrollArea className="h-64 rounded-lg border">
            <pre className="p-3 font-mono text-xs whitespace-pre-wrap break-all">{preview}</pre>
          </ScrollArea>
          <p className="text-xs text-muted-foreground">
            This is read-only. Nothing runs until you migrate and confirm.
          </p>
        </div>
      )}

      <Dialog open={confirmOpen} onOpenChange={(open) => !open && setConfirmOpen(false)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Migrate {selected.size} lookup(s) to {target}?</DialogTitle>
            <DialogDescription>
              This writes reference data from <strong>{source}</strong> into{" "}
              <strong>{target}</strong>. Existing values with the same Id are updated; missing ones
              are inserted. Rows already in the target that are not in the source are left alone.
            </DialogDescription>
          </DialogHeader>
          <Label className="flex items-center gap-2 font-normal">
            <Checkbox checked={!skipBackup} onCheckedChange={(checked) => setSkipBackup(!checked)} />
            Write a rollback script first (recommended)
          </Label>
          <div className="grid gap-2">
            <Label htmlFor="mig-confirm">
              Type <strong>{target || "the target environment"}</strong> to confirm
            </Label>
            <Input
              id="mig-confirm"
              value={confirmText}
              onChange={(event) => setConfirmText(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmOpen(false)}>Cancel</Button>
            <Button
              variant="destructive"
              disabled={!target || confirmText !== target}
              onClick={migrate}
            >
              Migrate reference data
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
