import ErrorNote from "../../lib/ErrorNote";
import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { EnvSummary, exportSql, listEnvironments, runSql, SqlResult } from "../../lib/ipc";

const SAMPLE = 'SELECT "Id", "Name", "CreatedOn"\nFROM "Contact"\nORDER BY "CreatedOn" DESC\nLIMIT 50';
const SAVED_QUERIES_KEY = "creatio-devhub.saved-sql-queries.v1";

interface SavedSqlQuery {
  id: string;
  name: string;
  env: string;
  query: string;
  updatedAt: number;
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
      if (res.rowCount === 0) setNotice("Query ran — 0 rows returned.");
    } catch (e) {
      setResult(null);
      setError(String(e));
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
      setError(String(e));
    } finally {
      setExporting(null);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      doRun();
    }
  };

  return (
    <div className="page-body">
      <div className="page-bar">
        <h1>SQL</h1>
        <div className="ws-actions">
          <button className="ghost" onClick={onShowJobs}>Jobs</button>
          <button className="ghost" onClick={() => doExport("csv")} disabled={!result || exporting !== null}>
            {exporting === "csv" ? "Exporting…" : "⭳ CSV"}
          </button>
          <button className="ghost" onClick={() => doExport("xlsx")} disabled={!result || exporting !== null}>
            {exporting === "xlsx" ? "Exporting…" : "⭳ Excel"}
          </button>
          <button className="primary" onClick={() => doRun()} disabled={!env || loading}>
            {loading ? "Running…" : "▶ Run"}
          </button>
        </div>
      </div>

      <div className="package-toolbar">
        <label>
          Environment
          <select value={env} onChange={(e) => setEnv(e.target.value)}>
            {envs.map((e) => (
              <option key={e.name} value={e.name}>
                {e.name} {e.isActive ? "(default)" : ""}
              </option>
            ))}
          </select>
        </label>
        <span className="package-count">Ctrl/⌘ + Enter to run</span>
      </div>

      <div className="sql-savebar">
        <label>
          Query name
          <input
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
        </label>
        <button className="ghost" onClick={saveCurrentQuery}>
          {activeSavedId ? "Update saved query" : "Save query"}
        </button>
        {activeSavedId && <button className="ghost" onClick={saveAsNew}>Save as new</button>}
      </div>

      <textarea
        className="sql-editor"
        value={query}
        spellCheck={false}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKey}
        placeholder='SELECT "Id", "Name" FROM "Contact" LIMIT 100'
      />

      <p className="hint">
        Runs raw SQL through clio (the environment needs the <code>cliogate</code> helper). Be careful with
        <code> UPDATE</code>/<code>DELETE</code> — it runs directly against the Creatio database. Export always
        writes the full result; the grid below shows up to 5,000 rows.
      </p>

      {notice && <p className="notice">{notice}</p>}
      {error && <ErrorNote error={error} />}

      {savedQueries.length > 0 && (
        <section className="sql-saved">
          <div className="sql-saved-heading">
            <div>
              <h2>Saved queries</h2>
              <p>Stored on this device. Open one to edit it or run it again immediately.</p>
            </div>
            <span className="pill dim">{savedQueries.length}</span>
          </div>
          <div className="sql-saved-list">
            {savedQueries.map((saved) => (
              <article className={`sql-saved-row ${activeSavedId === saved.id ? "active" : ""}`} key={saved.id}>
                <button className="sql-saved-main" onClick={() => openSavedQuery(saved)}>
                  <strong>{saved.name}</strong>
                  <span>{saved.env} · updated {new Date(saved.updatedAt).toLocaleString()}</span>
                  <code>{saved.query.replace(/\s+/g, " ").slice(0, 140)}</code>
                </button>
                <div className="sql-saved-actions">
                  <button className="primary" onClick={() => rerunSavedQuery(saved)} disabled={loading}>
                    Run again
                  </button>
                  <button className="ghost" onClick={() => openSavedQuery(saved)}>Open</button>
                  <button className="ghost danger-text" onClick={() => deleteSavedQuery(saved)}>Delete</button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {result && (
        <>
          <div className="sql-meta">
            <span className="pill accent">{result.rowCount.toLocaleString()} row{result.rowCount === 1 ? "" : "s"}</span>
            <span className="pill dim">{result.columns.length} column{result.columns.length === 1 ? "" : "s"}</span>
            {result.truncated && <span className="pill warn">grid shows first {result.rows.length.toLocaleString()}</span>}
          </div>
          {result.columns.length === 0 ? (
            <p className="empty">This statement returned no result set.</p>
          ) : (
            <div className="sql-grid">
              <table>
                <thead>
                  <tr>
                    <th className="rownum">#</th>
                    {result.columns.map((c, i) => (
                      <th key={i}>{c}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {result.rows.map((row, r) => (
                    <tr key={r}>
                      <td className="rownum">{r + 1}</td>
                      {result.columns.map((_, c) => (
                        <td key={c} title={row[c] ?? ""}>{row[c] ?? ""}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
