import { Fragment, useEffect, useMemo, useState } from "react";
import { ChevronRight, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import ErrorNote from "../../lib/ErrorNote";
import {
  captureLookups, deleteLookupSnapshot, DiffReport, DiffRow, diffLookups, EnvSummary,
  listEnvironments, listLookupSnapshots, LookupSnapshotInfo, onJobUpdate,
} from "../../lib/ipc";

const STATUS_LABEL: Record<string, string> = {
  different: "Differs",
  missingTarget: "Only in source",
  missingSource: "Only in target",
  same: "Same",
};

const age = (ms: number) => (ms ? new Date(ms).toLocaleString() : "never");

/** Read-only comparison of lookup (reference-data) contents between two
 *  environments. Parallel to the configuration comparison but on its own
 *  snapshots — `capture_lookups` reads every lookup's values into a local file,
 *  and the diff runs against those files, keyed on each value's Id. */
export default function LookupCompare({ onShowJobs }: { onShowJobs: () => void }) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [snapshots, setSnapshots] = useState<LookupSnapshotInfo[]>([]);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [report, setReport] = useState<DiffReport | null>(null);
  const [differencesOnly, setDifferencesOnly] = useState(true);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  const refreshSnapshots = () => listLookupSnapshots().then(setSnapshots).catch(() => {});

  useEffect(() => {
    listEnvironments()
      .then((list) => {
        setEnvs(list);
        setSource(list.find((item) => item.isActive)?.name ?? list[0]?.name ?? "");
        setTarget(list.find((item) => !item.isActive)?.name ?? "");
      })
      .catch((e) => setError(String(e)));
    refreshSnapshots();
  }, []);

  useEffect(() => {
    const un = onJobUpdate((job) => {
      if (job.kind === "capture-lookups" && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        refreshSnapshots();
        if (job.status === "succeeded") setNotice(`Captured lookups of ${job.env}.`);
      }
    });
    return () => { un.then((fn) => fn()); };
  }, []);

  const snapshotFor = (env: string) => snapshots.find((snap) => snap.env === env);

  const capture = async (env: string) => {
    setError("");
    setNotice("");
    try {
      await captureLookups(env);
      setNotice(
        `Reading every lookup's values in ${env} — follow it in Jobs. It reads the whole lookup set (a minute or two, longer for cloud) and writes nothing to the environment.`,
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const compare = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    setExpanded(new Set());
    try {
      setReport(await diffLookups(source, target));
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const forget = async (env: string) => {
    try {
      await deleteLookupSnapshot(env);
      await refreshSnapshots();
      setReport(null);
      setNotice(`Deleted the lookup snapshot of ${env}.`);
    } catch (e) {
      setError(String(e));
    }
  };

  const visible = useMemo(() => {
    if (!report) return [];
    return report.rows.filter((row) => !differencesOnly || row.status !== "same");
  }, [report, differencesOnly]);

  const toggle = (key: string) => {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key); else next.add(key);
    setExpanded(next);
  };

  const cell = (value: string | null) =>
    value === null
      ? <span className="text-muted-foreground">—</span>
      : <span className="text-sm">{value}</span>;

  return (
    <div className="grid gap-4">
      <p className="text-sm text-muted-foreground">
        Compares the values inside lookup tables — the reference data that config deployment does
        not carry unless it is bound to a package. Capture each side first; the comparison runs
        against those snapshots and writes to nothing.
      </p>

      <div className="grid gap-3 sm:grid-cols-2">
        {([["Source", source, setSource], ["Target", target, setTarget]] as const).map(
          ([label, value, set]) => (
            <div key={label} className="grid gap-2 rounded-lg border p-3">
              <Label htmlFor={`lk-${label}`}>{label}</Label>
              <Select value={value} onValueChange={set}>
                <SelectTrigger id={`lk-${label}`} className="w-full">
                  <SelectValue placeholder="Select an environment" />
                </SelectTrigger>
                <SelectContent>
                  {envs.map((item) => (
                    <SelectItem key={item.name} value={item.name}>{item.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Captured {age(snapshotFor(value)?.capturedAt ?? 0)}
                {snapshotFor(value) ? ` · ${snapshotFor(value)!.lookupCount} lookups` : ""}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" variant="outline" disabled={!value} onClick={() => capture(value)}>
                  {snapshotFor(value) ? "Re-capture" : "Capture"}
                </Button>
                {snapshotFor(value) && (
                  <Button size="sm" variant="ghost" onClick={() => forget(value)}>
                    <Trash2 className="size-4" aria-hidden="true" /> Delete
                  </Button>
                )}
              </div>
            </div>
          ),
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={!source || !target || source === target || busy} onClick={compare}>
          {busy ? "Comparing…" : "Compare lookups"}
        </Button>
        <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
        <Label className="ml-2 flex items-center gap-2 font-normal">
          <Checkbox
            checked={differencesOnly}
            onCheckedChange={(checked) => setDifferencesOnly(checked === true)}
          />
          Differences only
        </Label>
      </div>

      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
      {error && <ErrorNote error={error} />}

      {report && (
        <>
          <p className="text-sm text-muted-foreground">
            {report.counts.lookup
              ? `${report.counts.lookup} lookup${report.counts.lookup === 1 ? "" : "s"} differ between ${report.sourceEnv} and ${report.targetEnv}.`
              : `No lookups differ between ${report.sourceEnv} and ${report.targetEnv}.`}
          </p>

          {visible.length === 0 ? (
            <p className="text-muted-foreground">
              {differencesOnly ? "No differing lookups." : "No lookups captured."}
            </p>
          ) : (
            <div className="overflow-x-auto rounded-lg border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Lookup</TableHead>
                    <TableHead>{report.sourceEnv}</TableHead>
                    <TableHead>{report.targetEnv}</TableHead>
                    <TableHead className="w-36">Status</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {visible.map((row) => (
                    <Fragment key={row.key}>
                      <TableRow>
                        <TableCell className="font-medium">
                          <div className="flex items-center gap-1">
                            {row.detail.length > 0 && (
                              <button
                                onClick={() => toggle(row.key)}
                                className="text-muted-foreground hover:text-foreground"
                                aria-label={`Show differing values of ${row.key}`}
                              >
                                <ChevronRight
                                  className={`size-4 transition-transform ${expanded.has(row.key) ? "rotate-90" : ""}`}
                                  aria-hidden="true"
                                />
                              </button>
                            )}
                            {row.key}
                          </div>
                          {row.detail.length > 0 && (
                            <span className="text-xs text-muted-foreground">
                              {row.detail.length} value{row.detail.length === 1 ? "" : "s"} differ
                            </span>
                          )}
                        </TableCell>
                        <TableCell>{cell(row.source)}</TableCell>
                        <TableCell>{cell(row.target)}</TableCell>
                        <TableCell>
                          <Badge variant={row.status === "different" ? "destructive" : "secondary"}>
                            {STATUS_LABEL[row.status] ?? row.status}
                          </Badge>
                        </TableCell>
                      </TableRow>
                      {expanded.has(row.key) && row.detail.map((child: DiffRow) => (
                        <TableRow key={`${row.key}/${child.key}`} className="bg-muted/40">
                          <TableCell className="pl-10 font-mono text-xs break-all">{child.key}</TableCell>
                          <TableCell className="text-xs">{child.source ?? "—"}</TableCell>
                          <TableCell className="text-xs">{child.target ?? "—"}</TableCell>
                          <TableCell>
                            <Badge variant="outline">{STATUS_LABEL[child.status] ?? child.status}</Badge>
                          </TableCell>
                        </TableRow>
                      ))}
                    </Fragment>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
