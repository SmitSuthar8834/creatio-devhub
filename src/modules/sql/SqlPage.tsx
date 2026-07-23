import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { BookmarkX, CircleCheck, Download, Info, Play, TriangleAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import ErrorNote from "../../lib/ErrorNote";
import { EnvSummary, exportSql, listEnvironments, runSql, SqlResult } from "../../lib/ipc";

const SAMPLE = 'SELECT "Id", "Name", "CreatedOn"\nFROM "Contact"\nORDER BY "CreatedOn" DESC\nLIMIT 50';
const SAVED_QUERIES_KEY = "creatio-devhub.saved-sql-queries.v1";
const ERROR_LOG_KEY = "creatio-devhub.sql-error-log.v1";
const ERROR_LOG_LIMIT = 50;

interface SavedSqlQuery {
  id: string;
  name: string;
  env: string;
  query: string;
  updatedAt: number;
}

interface SqlErrorEntry {
  id: string;
  at: number;
  env: string;
  query: string;
  message: string;
  /** Whether the failure came from running the query or exporting its result. */
  kind: "run" | "export";
}

function readSavedQueries(): SavedSqlQuery[] {
  try {
    const value = JSON.parse(localStorage.getItem(SAVED_QUERIES_KEY) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter(
      (item): item is SavedSqlQuery =>
        typeof item?.id === "string" &&
        typeof item?.name === "string" &&
        typeof item?.env === "string" &&
        typeof item?.query === "string" &&
        typeof item?.updatedAt === "number",
    );
  } catch {
    return [];
  }
}

function readErrorLog(): SqlErrorEntry[] {
  try {
    const value = JSON.parse(localStorage.getItem(ERROR_LOG_KEY) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter(
      (item): item is SqlErrorEntry =>
        typeof item?.id === "string" &&
        typeof item?.at === "number" &&
        typeof item?.env === "string" &&
        typeof item?.query === "string" &&
        typeof item?.message === "string" &&
        (item?.kind === "run" || item?.kind === "export"),
    );
  } catch {
    return [];
  }
}

const newId = () => globalThis.crypto?.randomUUID?.() ?? `id-${Date.now()}-${Math.random()}`;

/**
 * Run SQL against a Creatio environment (via clio + cliogate) and export the
 * result to CSV or Excel. Read-heavy by nature — the grid caps at 5,000 rows,
 * but exports write the full result.
 */
export default function SqlPage({ onShowJobs }: { onShowJobs: () => void }) {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [env, setEnv] = useState("");
  const [query, setQuery] = useState(SAMPLE);
  const [result, setResult] = useState<SqlResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState<"csv" | "xlsx" | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [savedQueries, setSavedQueries] = useState<SavedSqlQuery[]>(readSavedQueries);
  const [savedName, setSavedName] = useState("");
  const [activeSavedId, setActiveSavedId] = useState<string | null>(null);
  const [errorLog, setErrorLog] = useState<SqlErrorEntry[]>(readErrorLog);
  const [tab, setTab] = useState("editor");

  const persistErrorLog = (next: SqlErrorEntry[]) => {
    const trimmed = next.slice(0, ERROR_LOG_LIMIT);
    setErrorLog(trimmed);
    localStorage.setItem(ERROR_LOG_KEY, JSON.stringify(trimmed));
  };

  // Keep a device-local history of failures so they collect in the Errors tab
  // instead of flashing once and disappearing.
  const recordError = (message: string, kind: SqlErrorEntry["kind"], forEnv = env, forQuery = query) => {
    setError(message);
    persistErrorLog([
      { id: newId(), at: Date.now(), env: forEnv, query: forQuery, message, kind },
      ...errorLog,
    ]);
  };

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvs(list);
      const active = list.find((e) => e.isActive) ?? list[0];
      if (active) setEnv(active.name);
    });
  }, []);

  const doRun = async (runEnv = env, runQuery = query) => {
    if (!runEnv || !runQuery.trim() || loading) return;
    setLoading(true);
    setError("");
    setNotice("");
    try {
      const res = await runSql(runEnv, runQuery);
      setResult(res);
      // Outcomes, and the wording has to match: a run with an explanatory note
      // (shown in its own panel), a statement that did its work, a query that
      // matched nothing, and a query with rows to show.
      if (res.messages.length > 0) setNotice("");
      else if (res.statement) setNotice("Statement ran successfully.");
      else if (res.rowCount === 0) setNotice("Query ran — 0 rows returned.");
    } catch (e) {
      setResult(null);
      recordError(String(e), "run", runEnv, runQuery);
    } finally {
      setLoading(false);
    }
  };

  const persistSavedQueries = (next: SavedSqlQuery[]) => {
    const ordered = [...next].sort((a, b) => b.updatedAt - a.updatedAt);
    setSavedQueries(ordered);
    localStorage.setItem(SAVED_QUERIES_KEY, JSON.stringify(ordered));
  };

  const saveCurrentQuery = () => {
    const name = savedName.trim();
    if (!name) {
      setError("Enter a name for this query.");
      return;
    }
    if (!env || !query.trim()) {
      setError("Choose an environment and enter a SQL query.");
      return;
    }

    const id = activeSavedId ?? globalThis.crypto?.randomUUID?.() ?? `query-${Date.now()}`;
    const saved: SavedSqlQuery = { id, name, env, query: query.trim(), updatedAt: Date.now() };
    persistSavedQueries([saved, ...savedQueries.filter((item) => item.id !== id)]);
    setActiveSavedId(id);
    setSavedName(name);
    setError("");
    setNotice(activeSavedId ? `Updated “${name}”.` : `Saved “${name}”.`);
  };

  const openSavedQuery = (saved: SavedSqlQuery) => {
    const environmentAvailable = envs.some((item) => item.name === saved.env);
    setActiveSavedId(saved.id);
    setSavedName(saved.name);
    if (environmentAvailable) setEnv(saved.env);
    setQuery(saved.query);
    setResult(null);
    setError("");
    setNotice(
      environmentAvailable
        ? `Opened “${saved.name}”.`
        : `Opened “${saved.name}”. Its saved environment “${saved.env}” is not currently registered.`,
    );
  };

  const rerunSavedQuery = async (saved: SavedSqlQuery) => {
    if (!envs.some((item) => item.name === saved.env)) {
      setError(`The saved environment “${saved.env}” is not registered.`);
      return;
    }
    const runEnv = saved.env;
    setActiveSavedId(saved.id);
    setSavedName(saved.name);
    setEnv(runEnv);
    setQuery(saved.query);
    await doRun(runEnv, saved.query);
  };

  const deleteSavedQuery = (saved: SavedSqlQuery) => {
    if (!window.confirm(`Delete the saved query “${saved.name}”?`)) return;
    persistSavedQueries(savedQueries.filter((item) => item.id !== saved.id));
    if (activeSavedId === saved.id) {
      setActiveSavedId(null);
      setSavedName("");
    }
    setNotice(`Deleted “${saved.name}”.`);
  };

  const saveAsNew = () => {
    setActiveSavedId(null);
    setSavedName("");
    setNotice("Enter a new name to save another copy.");
  };

  const doExport = async (format: "csv" | "xlsx") => {
    if (!env || !query.trim()) return;
    setError("");
    setNotice("");
    try {
      const path = await save({
        defaultPath: `creatio-query.${format}`,
        filters: [
          format === "xlsx"
            ? { name: "Excel workbook", extensions: ["xlsx"] }
            : { name: "CSV (semicolon-separated)", extensions: ["csv"] },
        ],
      });
      if (typeof path !== "string") return; // cancelled
      setExporting(format);
      await exportSql({ env, query, format, path });
      setNotice(`Exported to ${path}`);
    } catch (e) {
      recordError(String(e), "export");
    } finally {
      setExporting(null);
    }
  };

  const openErrorQuery = (entry: SqlErrorEntry) => {
    if (envs.some((item) => item.name === entry.env)) setEnv(entry.env);
    setQuery(entry.query);
    setResult(null);
    setError("");
    setNotice(`Loaded the query from the ${new Date(entry.at).toLocaleString()} error.`);
    setTab("editor");
  };

  const dismissError = (entry: SqlErrorEntry) => {
    persistErrorLog(errorLog.filter((item) => item.id !== entry.id));
  };

  const clearErrorLog = () => {
    if (errorLog.length === 0) return;
    if (!window.confirm(`Clear all ${errorLog.length} logged error${errorLog.length === 1 ? "" : "s"}?`)) return;
    persistErrorLog([]);
  };

  // An UPDATE/INSERT/DDL that worked: it reports success rather than a grid.
  const statementSucceeded = !!result && result.statement;
  // Only a query that actually returned rows has something to show or export.
  const hasGrid = !!result && result.columns.length > 0;

  const onKey = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      doRun();
    }
  };

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">SQL</h1>
        <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
      </div>

      <Tabs value={tab} onValueChange={setTab} className="gap-4">
        <TabsList>
          <TabsTrigger value="editor">Editor</TabsTrigger>
          <TabsTrigger value="saved">
            Saved queries
            {savedQueries.length > 0 && (
              <Badge variant="secondary" className="ml-1">{savedQueries.length}</Badge>
            )}
          </TabsTrigger>
          <TabsTrigger value="errors">
            Errors
            {errorLog.length > 0 && (
              <Badge className="ml-1 border-transparent bg-destructive/15 text-destructive">
                {errorLog.length}
              </Badge>
            )}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="editor" className="grid gap-4">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div className="grid min-w-56 gap-2">
              <Label htmlFor="sql-env">Environment</Label>
              <Select value={env} onValueChange={setEnv}>
                <SelectTrigger id="sql-env" className="w-full">
                  <SelectValue placeholder="Select an environment" />
                </SelectTrigger>
                <SelectContent>
                  {envs.map((e) => (
                    <SelectItem key={e.name} value={e.name}>
                      {e.name} {e.isActive ? "(default)" : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <span className="mr-1 text-sm text-muted-foreground">Ctrl/⌘ + Enter to run</span>
              <Button
                variant="outline"
                onClick={() => doExport("csv")}
                disabled={!hasGrid || exporting !== null}
                title={result && !hasGrid ? "There are no rows to export." : undefined}
              >
                <Download aria-hidden="true" />
                {exporting === "csv" ? "Exporting…" : "CSV"}
              </Button>
              <Button
                variant="outline"
                onClick={() => doExport("xlsx")}
                disabled={!hasGrid || exporting !== null}
                title={result && !hasGrid ? "There are no rows to export." : undefined}
              >
                <Download aria-hidden="true" />
                {exporting === "xlsx" ? "Exporting…" : "Excel"}
              </Button>
              <Button onClick={() => doRun()} disabled={!env || loading}>
                <Play aria-hidden="true" />
                {loading ? "Running…" : "Run"}
              </Button>
            </div>
          </div>

          <div className="flex flex-wrap items-end gap-2">
            <div className="grid min-w-64 flex-1 gap-2">
              <Label htmlFor="sql-name">Query name</Label>
              <Input
                id="sql-name"
                value={savedName}
                onChange={(e) => setSavedName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    saveCurrentQuery();
                  }
                }}
                placeholder="e.g. Recently created contacts"
              />
            </div>
            <Button variant="outline" onClick={saveCurrentQuery}>
              {activeSavedId ? "Update saved query" : "Save query"}
            </Button>
            {activeSavedId && <Button variant="ghost" onClick={saveAsNew}>Save as new</Button>}
          </div>

          <Textarea
            className="min-h-40 font-mono text-sm"
            value={query}
            spellCheck={false}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder='SELECT "Id", "Name" FROM "Contact" LIMIT 100'
          />

          <p className="text-sm text-muted-foreground">
            Runs raw SQL through clio (the environment needs the{" "}
            <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">cliogate</code> helper).
            Be careful with{" "}
            <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">UPDATE</code>/
            <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">DELETE</code> — it runs
            directly against the Creatio database. Export always writes the full result; the grid below
            shows up to 5,000 rows.
          </p>

          {notice && (
            statementSucceeded
              ? (
                <p className="flex items-center gap-2 rounded-md border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
                  <CircleCheck className="size-4 shrink-0" aria-hidden="true" />
                  {notice}
                </p>
              )
              : <p className="text-sm text-muted-foreground">{notice}</p>
          )}
          {error && (
            <div className="grid gap-1">
              <ErrorNote error={error} />
              <p className="text-xs text-muted-foreground">
                Also saved to the Errors tab so you can find it later.
              </p>
            </div>
          )}

          {result && result.messages.length > 0 && (
            <div className="grid gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2.5">
              {result.messages.map((message, i) => (
                <p key={i} className="flex items-start gap-2 text-sm text-foreground">
                  <Info className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
                  <span>{message}</span>
                </p>
              ))}
            </div>
          )}

          {hasGrid && (
            <div className="grid gap-3">
              <div className="flex flex-wrap gap-2">
                <Badge className="border-transparent bg-accent/15 text-accent-foreground">
                  {result.rowCount.toLocaleString()} row{result.rowCount === 1 ? "" : "s"}
                </Badge>
                <Badge variant="secondary">
                  {result.columns.length} column{result.columns.length === 1 ? "" : "s"}
                </Badge>
                {result.truncated && (
                  <Badge className="border-transparent bg-warning/15 text-warning">
                    grid shows first {result.rows.length.toLocaleString()}
                  </Badge>
                )}
              </div>
              {/* A result with no columns is a statement, reported above as success. */}
              <div className="max-h-[60vh] overflow-auto rounded-lg border">
                <Table>
                  <TableHeader className="sticky top-0 z-10 bg-card">
                    <TableRow>
                      <TableHead className="w-12 text-right text-muted-foreground">#</TableHead>
                      {result.columns.map((c, i) => (
                        <TableHead key={i} className="whitespace-nowrap">{c}</TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {result.rows.map((row, r) => (
                      <TableRow key={r}>
                        <TableCell className="text-right font-mono text-xs text-muted-foreground">
                          {r + 1}
                        </TableCell>
                        {result.columns.map((_, c) => (
                          <TableCell key={c} className="max-w-80 truncate" title={row[c] ?? ""}>
                            {row[c] ?? ""}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </div>
          )}
        </TabsContent>

        <TabsContent value="saved" className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h2 className="text-base font-semibold">Saved queries</h2>
              <p className="text-sm text-muted-foreground">
                Stored on this device. Open one to edit it or run it again immediately.
              </p>
            </div>
            {savedQueries.length > 0 && <Badge variant="secondary">{savedQueries.length}</Badge>}
          </div>

          {savedQueries.length === 0
            ? (
              <div className="grid justify-items-center gap-1 rounded-lg border border-dashed p-8 text-center">
                <BookmarkX className="size-6 text-muted-foreground" aria-hidden="true" />
                <p className="text-sm font-medium">No saved queries yet</p>
                <p className="text-sm text-muted-foreground">
                  Name a query in the Editor tab and choose <em>Save query</em> to keep it here.
                </p>
              </div>
            )
            : (
              <div className="grid gap-2">
                {savedQueries.map((saved) => (
                  <article
                    key={saved.id}
                    className={cn(
                      "flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3",
                      activeSavedId === saved.id && "border-primary bg-accent/10",
                    )}
                  >
                    <button
                      className="grid min-w-0 flex-1 gap-0.5 text-left"
                      onClick={() => { openSavedQuery(saved); setTab("editor"); }}
                    >
                      <strong className="text-sm">{saved.name}</strong>
                      <span className="text-xs text-muted-foreground">
                        {saved.env} · updated {new Date(saved.updatedAt).toLocaleString()}
                      </span>
                      <code className="truncate font-mono text-xs text-muted-foreground">
                        {saved.query.replace(/\s+/g, " ").slice(0, 140)}
                      </code>
                    </button>
                    <div className="flex flex-wrap gap-2">
                      <Button size="sm" onClick={() => { rerunSavedQuery(saved); setTab("editor"); }} disabled={loading}>
                        Run again
                      </Button>
                      <Button size="sm" variant="outline" onClick={() => { openSavedQuery(saved); setTab("editor"); }}>
                        Open
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-destructive hover:text-destructive"
                        onClick={() => deleteSavedQuery(saved)}
                      >
                        Delete
                      </Button>
                    </div>
                  </article>
                ))}
              </div>
            )}
        </TabsContent>

        <TabsContent value="errors" className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h2 className="text-base font-semibold">Errors</h2>
              <p className="text-sm text-muted-foreground">
                Failed runs and exports, most recent first. Stored on this device (last {ERROR_LOG_LIMIT}).
              </p>
            </div>
            {errorLog.length > 0 && (
              <Button variant="outline" size="sm" onClick={clearErrorLog}>Clear all</Button>
            )}
          </div>

          {errorLog.length === 0
            ? (
              <div className="grid justify-items-center gap-1 rounded-lg border border-dashed p-8 text-center">
                <CircleCheck className="size-6 text-success" aria-hidden="true" />
                <p className="text-sm font-medium">No errors logged</p>
                <p className="text-sm text-muted-foreground">
                  When a run or export fails, it will be collected here.
                </p>
              </div>
            )
            : (
              <div className="grid gap-3">
                {errorLog.map((entry) => (
                  <article key={entry.id} className="grid gap-2 rounded-lg border p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex flex-wrap items-center gap-2">
                        <TriangleAlert className="size-4 text-destructive" aria-hidden="true" />
                        <Badge variant="secondary">{entry.kind === "export" ? "Export" : "Run"}</Badge>
                        <span className="text-xs text-muted-foreground">
                          {entry.env || "no environment"} · {new Date(entry.at).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <Button size="sm" variant="outline" onClick={() => openErrorQuery(entry)}>
                          Open query
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-destructive hover:text-destructive"
                          onClick={() => dismissError(entry)}
                        >
                          Dismiss
                        </Button>
                      </div>
                    </div>
                    <ErrorNote error={entry.message} />
                    <code className="truncate font-mono text-xs text-muted-foreground">
                      {entry.query.replace(/\s+/g, " ").slice(0, 140) || "(empty query)"}
                    </code>
                  </article>
                ))}
              </div>
            )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
