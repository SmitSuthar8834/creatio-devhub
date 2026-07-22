import { useEffect, useState } from "react";
import { AlertTriangle, ChevronDown, DatabaseZap } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent,
  AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import ErrorNote from "../../lib/ErrorNote";
import {
  analyzeContent, ContentEntityResult, ContentGapReport, ContentMigrateReport,
  ContentRecordPick, EnvSummary, finalizeContent, FlowMigrateReport, listContentRecords,
  listEnvironments, migrateContent, migrateContentFlows, verifyContent,
} from "../../lib/ipc";

const BASE = ["BfEmailTemplate", "DCTemplate", "DCReplica", "Campaign", "BulkEmail"];
/** Entities the user can narrow to individual records. */
const PICKABLE = ["Campaign", "BulkEmail"];

function GapTable({ report }: { report: ContentGapReport }) {
  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader><TableRow><TableHead>Entity</TableHead><TableHead>Source</TableHead><TableHead>Target</TableHead><TableHead>Missing</TableHead><TableHead>Notes</TableHead></TableRow></TableHeader>
        <TableBody>{report.entities.map((row) => (
          <TableRow key={row.entity}>
            <TableCell className="font-mono text-xs">{row.entity}</TableCell><TableCell>{row.sourceCount}</TableCell><TableCell>{row.targetCount}</TableCell>
            <TableCell><Badge variant={row.missingCount ? "destructive" : "secondary"}>{row.missingCount}</Badge></TableCell>
            <TableCell className="text-xs text-muted-foreground">{row.notes.join(" · ") || "—"}</TableCell>
          </TableRow>
        ))}</TableBody>
      </Table>
    </div>
  );
}

function Results({ rows }: { rows: ContentEntityResult[] }) {
  return <div className="grid gap-2">{rows.map((row) => {
    const adjustments = row.adjustments ?? [];
    const detailCount = row.failures.length + adjustments.length;
    return (
      <Collapsible key={row.entity} className="rounded-lg border p-3">
        <div className="flex items-center gap-2"><strong className="font-mono text-sm">{row.entity}</strong><Badge variant="secondary">{row.inserted} inserted</Badge><span className="text-xs text-muted-foreground">{row.skipped} skipped{row.notSelected ? ` · ${row.notSelected} not selected` : ""}</span>
          {detailCount > 0 && <CollapsibleTrigger asChild><Button className="ml-auto" size="sm" variant="ghost"><ChevronDown data-icon="inline-start" />{row.failures.length ? `${row.failures.length} failures` : ""}{row.failures.length && adjustments.length ? " · " : ""}{adjustments.length ? `${adjustments.length} adjustments` : ""}</Button></CollapsibleTrigger>}
        </div>
        <CollapsibleContent className="mt-2 grid gap-2">
          {adjustments.map((adjustment, index) => <p key={`${adjustment.id}-${adjustment.column}-${index}`} className="break-all text-xs text-warning">{adjustment.name || adjustment.id}{adjustment.column ? ` · ${adjustment.column}` : ""} · {adjustment.action} — {adjustment.detail}</p>)}
          {row.failures.map((failure) => <p key={failure.id} className="break-all text-xs text-destructive">{failure.name || failure.id} · {failure.status ? `HTTP ${failure.status} · ` : ""}{failure.error}</p>)}
        </CollapsibleContent>
      </Collapsible>
    );
  })}</div>;
}

export default function ContentMigration() {
  const [envs, setEnvs] = useState<EnvSummary[]>([]); const [source, setSource] = useState(""); const [target, setTarget] = useState("");
  const [selected, setSelected] = useState(new Set(BASE)); const [report, setReport] = useState<ContentGapReport | null>(null); const [result, setResult] = useState<ContentMigrateReport | null>(null); const [flowResult, setFlowResult] = useState<FlowMigrateReport | null>(null);
  const [picks, setPicks] = useState<Record<string, ContentRecordPick[]>>({});
  const [pickErrors, setPickErrors] = useState<Record<string, string>>({});
  /** Per entity: undefined = migrate every missing record; a Set = only these ids. */
  const [chosen, setChosen] = useState<Record<string, Set<string> | undefined>>({});
  const [busy, setBusy] = useState(""); const [error, setError] = useState(""); const [finalizeOpen, setFinalizeOpen] = useState(false); const same = !source || !target || source === target;
  useEffect(() => { listEnvironments().then((list) => { setEnvs(list); setSource(list.find((e) => e.isActive)?.name ?? list[0]?.name ?? ""); setTarget(list.find((e) => !e.isActive)?.name ?? ""); }).catch((e) => setError(String(e))); }, []);
  useEffect(() => { setPicks({}); setChosen({}); setPickErrors({}); }, [source, target]);
  const run = async <T,>(label: string, action: () => Promise<T>, done: (value: T) => void) => { setBusy(label); setError(""); try { done(await action()); } catch (e) { setError(String(e)); } finally { setBusy(""); } };
  const toggle = (entity: string) => { const next = new Set(selected); next.has(entity) ? next.delete(entity) : next.add(entity); setSelected(next); };
  const loadPicks = (entity: string) => {
    if (picks[entity] || same) return;
    listContentRecords(source, target, entity)
      .then((rows) => { setPicks((prev) => ({ ...prev, [entity]: rows })); setPickErrors((prev) => ({ ...prev, [entity]: "" })); })
      .catch((e) => setPickErrors((prev) => ({ ...prev, [entity]: String(e) })));
  };
  const missingOf = (entity: string) => (picks[entity] ?? []).filter((pick) => !pick.existsInTarget);
  const isChosen = (entity: string, id: string) => chosen[entity]?.has(id) ?? true;
  const chosenCount = (entity: string) => chosen[entity]?.size ?? missingOf(entity).length;
  const toggleRecord = (entity: string, id: string) => setChosen((prev) => {
    const current = new Set(prev[entity] ?? missingOf(entity).map((pick) => pick.id));
    current.has(id) ? current.delete(id) : current.add(id);
    return { ...prev, [entity]: current };
  });
  const chooseAll = (entity: string, all: boolean) => setChosen((prev) => ({ ...prev, [entity]: all ? undefined : new Set<string>() }));
  const migrate = () => {
    const selections: Record<string, string[]> = {};
    for (const entity of PICKABLE) { const set = chosen[entity]; if (set && selected.has(entity)) selections[entity] = [...set]; }
    return migrateContent(source, target, [...selected], Object.keys(selections).length ? selections : undefined);
  };
  return (
    <div className="grid gap-4 pt-2">
      <div><h1 className="text-xl font-semibold tracking-tight">Migrate marketing content</h1><p className="mt-1 text-sm text-muted-foreground">Copy campaigns, bulk emails, designer templates, dynamic content, and campaign flow diagrams with their original IDs.</p></div>
      <div className="grid gap-3 sm:grid-cols-2">{[["content-source", "Source (read from)", source, setSource], ["content-target", "Target (write to)", target, setTarget]].map(([id, title, value, setter]) => <div key={id as string} className="grid gap-2 rounded-lg border p-3"><Label htmlFor={id as string}>{title as string}</Label><Select value={value as string} onValueChange={setter as (value: string) => void}><SelectTrigger id={id as string} className="w-full"><SelectValue placeholder="Select an environment" /></SelectTrigger><SelectContent>{envs.map((env) => <SelectItem key={env.name} value={env.name}>{env.name}</SelectItem>)}</SelectContent></Select></div>)}</div>
      {same && source && target && <p className="text-sm text-destructive">Choose two different environments.</p>}
      {error && <ErrorNote error={error} />}
      <div className="flex flex-wrap gap-2"><Button disabled={same || !!busy} onClick={() => run("Analyzing…", () => analyzeContent(source, target), setReport)}>{busy === "Analyzing…" ? busy : "Analyze"}</Button><Button variant="outline" disabled={same || !!busy} onClick={() => run("Verifying…", () => verifyContent(source, target), setReport)}>Verify</Button></div>
      {report && <><GapTable report={report} />
        {report.brokenFlows.length > 0 && <Alert><DatabaseZap /><AlertTitle>Flow diagrams are missing on the target</AlertTitle><AlertDescription>{report.brokenFlows.join(", ")}</AlertDescription></Alert>}
        {report.localImageReferences.length > 0 && <Alert variant="destructive"><AlertTriangle /><AlertTitle>Local images detected</AlertTitle><AlertDescription>These references are not migrated and may break: {report.localImageReferences.join(", ")}</AlertDescription></Alert>}
      </>}
      <div className="grid gap-3 rounded-lg border p-4"><div><h2 className="font-medium">Content records</h2><p className="text-sm text-muted-foreground">Parents are always copied before their dependents. Existing IDs are skipped, and references to missing lookup rows are matched by name or cleared — every change is reported.</p></div><div className="flex flex-wrap gap-4">{BASE.map((entity) => <Label key={entity} className="flex items-center gap-2 font-normal"><Checkbox checked={selected.has(entity)} onCheckedChange={() => toggle(entity)} />{entity}</Label>)}</div>
        {PICKABLE.filter((entity) => selected.has(entity)).map((entity) => {
          const missing = missingOf(entity);
          return (
            <Collapsible key={entity} className="rounded-lg border" onOpenChange={(open) => open && loadPicks(entity)}>
              <CollapsibleTrigger asChild>
                <Button variant="ghost" className="w-full justify-between font-normal">
                  <span>Choose {entity} records…</span>
                  <span className="flex items-center gap-2 text-xs text-muted-foreground">{picks[entity] ? `${chosenCount(entity)} of ${missing.length} missing selected` : "all missing records"}<ChevronDown /></span>
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="grid gap-1 border-t p-3">
                {pickErrors[entity] && <ErrorNote error={pickErrors[entity]} />}
                {!picks[entity] && !pickErrors[entity] && <p className="text-sm text-muted-foreground">Loading records…</p>}
                {picks[entity] && <>
                  <div className="mb-1 flex gap-2"><Button size="sm" variant="outline" onClick={() => chooseAll(entity, true)}>All missing</Button><Button size="sm" variant="outline" onClick={() => chooseAll(entity, false)}>None</Button></div>
                  {picks[entity].length === 0 && <p className="text-sm text-muted-foreground">The source has no {entity} records.</p>}
                  {picks[entity].map((pick) => (
                    <Label key={pick.id} className="flex items-center gap-2 py-0.5 font-normal">
                      <Checkbox disabled={pick.existsInTarget} checked={pick.existsInTarget ? false : isChosen(entity, pick.id)} onCheckedChange={() => toggleRecord(entity, pick.id)} />
                      <span className="min-w-0 truncate text-sm">{pick.name || pick.id}</span>
                      {pick.existsInTarget && <Badge variant="secondary">on target</Badge>}
                    </Label>
                  ))}
                </>}
              </CollapsibleContent>
            </Collapsible>
          );
        })}
        <Button className="w-fit" disabled={same || !selected.size || !!busy} onClick={() => run("Migrating content…", migrate, setResult)}>{busy === "Migrating content…" ? busy : "Migrate selected content"}</Button>{result && <><p className="text-xs text-muted-foreground">Rollback: {result.rollbackPath}</p><Results rows={result.entities} /></>}</div>
      <div className="grid gap-3 rounded-lg border p-4"><div><h2 className="font-medium">Flow diagrams</h2><p className="text-sm text-muted-foreground">Writes protected SysSchema rows through SQL, then copies versions, items, and localized captions.</p></div><Button variant="destructive" className="w-fit" disabled={same || !!busy} onClick={() => run("Migrating flows…", () => migrateContentFlows(source, target), setFlowResult)}>{busy === "Migrating flows…" ? busy : "Migrate flow diagrams (uses SQL)"}</Button>{flowResult && <><p className="text-sm">{flowResult.schemasInserted} schemas inserted · {flowResult.schemasSkipped} already present · {flowResult.campaignsRepointed} campaigns linked</p><p className="text-xs text-muted-foreground">Rollback: {flowResult.rollbackPath}</p><Results rows={flowResult.entities} /></>}</div>
      <div className="flex items-center justify-between gap-3 rounded-lg border p-4"><div><h2 className="font-medium">Finalize target</h2><p className="text-sm text-muted-foreground">Clear Redis and restart the web app so Creatio sees the new flows. This signs users out; a local IIS cold start can take several minutes.</p></div><Button variant="destructive" disabled={same || !!busy} onClick={() => setFinalizeOpen(true)}>Finalize…</Button></div>
      <AlertDialog open={finalizeOpen} onOpenChange={setFinalizeOpen}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Restart {target}?</AlertDialogTitle><AlertDialogDescription>This clears its Redis database and restarts the application. Active users will be disconnected and the cold start can take several minutes.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>Cancel</AlertDialogCancel><AlertDialogAction onClick={() => { setFinalizeOpen(false); run("Finalizing…", () => finalizeContent(target), () => undefined); }}>Clear cache and restart</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
    </div>
  );
}
