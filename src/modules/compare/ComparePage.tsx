import { Fragment, useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { ChevronRight, Eye, EyeOff, ShieldAlert, Trash2 } from "lucide-react";
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import ErrorNote from "../../lib/ErrorNote";
import {
  captureEnvState, deleteSnapshot, DiffReport, DiffRow, diffEnvironments, EnvSummary,
  exportDiffReport, listEnvironments, listSnapshots, onJobUpdate, SnapshotInfo,
} from "../../lib/ipc";

const CATEGORIES = [
  { id: "package", label: "Packages" },
  { id: "setting", label: "Settings" },
  { id: "feature", label: "Features" },
  { id: "webservice", label: "Web services" },
] as const;

const STATUS_LABEL: Record<string, string> = {
  different: "Differs",
  missingTarget: "Only in source",
  missingSource: "Only in target",
  same: "Same",
};

const age = (ms: number) => (ms ? new Date(ms).toLocaleString() : "never");

const duration = (ms: number) => {
  const total = Math.round(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return minutes ? `${minutes}m ${seconds}s` : `${seconds}s`;
};

/** What to tell someone before they commit to a wait.
 *
 *  A capture reads the whole configuration, and how long that takes is not
 *  guessable from here — a local install answers in a couple of minutes while a
 *  cloud tenant takes considerably longer. So the estimate is the environment's
 *  own previous capture whenever there is one, and an honest range before that. */
const expectation = (snap: SnapshotInfo | undefined) =>
  snap?.durationMs
    ? `Last capture took ${duration(snap.durationMs)}.`
    : "First capture reads the whole configuration — expect a few minutes, longer for cloud environments.";

export default function ComparePage({ onShowJobs }: { onShowJobs: () => void }) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [snapshots, setSnapshots] = useState<SnapshotInfo[]>([]);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [report, setReport] = useState<DiffReport | null>(null);
  const [category, setCategory] = useState<string>("package");
  const [differencesOnly, setDifferencesOnly] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  /** Setting values are masked until explicitly revealed, per row. */
  const [revealed, setRevealed] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const refreshSnapshots = () => listSnapshots().then(setSnapshots).catch(() => {});

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

  // A capture writes the snapshot file, so the ages are stale until it lands.
  useEffect(() => {
    const un = onJobUpdate((job) => {
      if (job.kind === "capture-state" && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        refreshSnapshots();
        if (job.status === "succeeded") setNotice(`Captured ${job.env}.`);
      }
    });
    return () => { un.then((fn) => fn()); };
  }, []);

  const snapshotFor = (env: string) => snapshots.find((snap) => snap.env === env);

  const capture = async (env: string) => {
    setError("");
    setNotice("");
    try {
      const previous = snapshotFor(env);
      await captureEnvState(env);
      setNotice(
        previous?.durationMs
          ? `Reading ${env}. It took ${duration(previous.durationMs)} last time — follow it in Jobs. It can be cancelled; nothing is written to the environment.`
          : `Reading ${env}. A first capture reads the whole configuration and can take several minutes — follow it in Jobs. It can be cancelled; nothing is written to the environment.`,
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const compare = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    setRevealed(new Set());
    try {
      setReport(await diffEnvironments(source, target));
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const forget = async (env: string) => {
    try {
      await deleteSnapshot(env);
      await refreshSnapshots();
      setReport(null);
      setNotice(`Deleted the snapshot of ${env}.`);
    } catch (e) {
      setError(String(e));
    }
  };

  const exportReport = async () => {
    const path = await save({
      title: "Save comparison report",
      defaultPath: `${source}-vs-${target}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (typeof path !== "string") return;
    try {
      await exportDiffReport(source, target, path);
      setNotice("Report saved. Setting values were omitted deliberately.");
    } catch (e) {
      setError(String(e));
    }
  };

  const visible = useMemo(() => {
    if (!report) return [];
    return report.rows.filter(
      (row) => row.category === category && (!differencesOnly || row.status !== "same"),
    );
  }, [report, category, differencesOnly]);

  const sensitiveCount = useMemo(
    () => visible.filter((row) => row.sensitive).length,
    [visible],
  );

  const toggle = (set: Set<string>, key: string, apply: (next: Set<string>) => void) => {
    const next = new Set(set);
    if (next.has(key)) next.delete(key); else next.add(key);
    apply(next);
  };

  /** Settings can hold API keys, so their values never render until asked for. */
  const cell = (row: DiffRow, value: string | null) => {
    if (value === null) return <span className="text-muted-foreground">—</span>;
    if (row.category !== "setting" || revealed.has(row.key)) {
      return <span className="font-mono text-xs break-all">{value || "(empty)"}</span>;
    }
    return <span className="text-muted-foreground">••••••</span>;
  };

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Compare environments</h1>
        <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
      </div>

      <p className="text-sm text-muted-foreground">
        Comparison runs against saved snapshots, not live environments — capture each side
        first. Nothing here writes to any environment.
      </p>

      <div className="grid gap-3 sm:grid-cols-2">
        {([["Source", source, setSource], ["Target", target, setTarget]] as const).map(
          ([label, value, set]) => (
            <div key={label} className="grid gap-2 rounded-lg border p-3">
              <Label htmlFor={`cmp-${label}`}>{label}</Label>
              <Select value={value} onValueChange={set}>
                <SelectTrigger id={`cmp-${label}`} className="w-full">
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
              </p>
              <p className="text-xs text-muted-foreground">{expectation(snapshotFor(value))}</p>
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
        <Button
          disabled={!source || !target || source === target || busy}
          onClick={compare}
        >
          {busy ? "Comparing…" : "Compare"}
        </Button>
        {report && (
          <Button variant="outline" onClick={exportReport}>Export report…</Button>
        )}
        <Label className="ml-2 flex items-center gap-2 font-normal">
          <Checkbox
            checked={differencesOnly}
            onCheckedChange={(checked) => setDifferencesOnly(checked === true)}
          />
          Differences only
        </Label>
      </div>

      <p className="flex items-start gap-2 rounded-lg border border-dashed p-3 text-xs text-muted-foreground">
        <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <span>
          Snapshots store system setting values as captured, which can include API keys and
          passwords. Values stay hidden until you reveal them, exported reports omit them
          entirely, and Delete removes the stored file.
        </span>
      </p>

      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
      {error && <ErrorNote error={error} />}

      {report && (
        <>
          <Tabs value={category} onValueChange={setCategory}>
            <TabsList>
              {CATEGORIES.map((item) => (
                <TabsTrigger key={item.id} value={item.id}>
                  {item.label}
                  {report.counts[item.id] ? (
                    <Badge variant="secondary" className="ml-2">{report.counts[item.id]}</Badge>
                  ) : null}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>

          {sensitiveCount > 0 && (
            <p className="text-xs text-muted-foreground">
              {sensitiveCount} of these look credential-shaped by name. That check is a hint,
              not a guarantee — treat every setting value as sensitive.
            </p>
          )}

          {visible.length === 0 ? (
            <p className="text-muted-foreground">
              {differencesOnly
                ? "No differences in this category."
                : "Nothing in this category."}
            </p>
          ) : (
            <div className="overflow-x-auto rounded-lg border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Item</TableHead>
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
                                onClick={() => toggle(expanded, row.key, setExpanded)}
                                className="text-muted-foreground hover:text-foreground"
                                aria-label={`Show schemas of ${row.key}`}
                              >
                                <ChevronRight
                                  className={`size-4 transition-transform ${
                                    expanded.has(row.key) ? "rotate-90" : ""
                                  }`}
                                  aria-hidden="true"
                                />
                              </button>
                            )}
                            {row.key}
                            {row.sensitive && (
                              <ShieldAlert
                                className="size-3.5 text-muted-foreground"
                                aria-label="Looks credential-shaped"
                              />
                            )}
                          </div>
                          {row.detail.length > 0 && (
                            <span className="text-xs text-muted-foreground">
                              {row.detail.length} schema{row.detail.length === 1 ? "" : "s"} differ
                            </span>
                          )}
                        </TableCell>
                        <TableCell>{cell(row, row.source)}</TableCell>
                        <TableCell>{cell(row, row.target)}</TableCell>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <Badge variant={row.status === "different" ? "destructive" : "secondary"}>
                              {STATUS_LABEL[row.status] ?? row.status}
                            </Badge>
                            {row.category === "setting" && (
                              <button
                                onClick={() => toggle(revealed, row.key, setRevealed)}
                                className="text-muted-foreground hover:text-foreground"
                                aria-label={`${revealed.has(row.key) ? "Hide" : "Reveal"} value of ${row.key}`}
                              >
                                {revealed.has(row.key)
                                  ? <EyeOff className="size-4" aria-hidden="true" />
                                  : <Eye className="size-4" aria-hidden="true" />}
                              </button>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                      {expanded.has(row.key) && row.detail.map((child) => (
                        <TableRow key={`${row.key}/${child.key}`} className="bg-muted/40">
                          <TableCell className="pl-10 text-sm">{child.key}</TableCell>
                          <TableCell className="font-mono text-xs">{child.source ?? "—"}</TableCell>
                          <TableCell className="font-mono text-xs">{child.target ?? "—"}</TableCell>
                          <TableCell>
                            <Badge variant="outline">
                              {STATUS_LABEL[child.status] ?? child.status}
                            </Badge>
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
