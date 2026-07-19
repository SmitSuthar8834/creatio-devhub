import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { EnvSummary, exportSql, listEnvironments, runSql, SqlResult } from "../../lib/ipc";

const SAMPLE = 'SELECT "Id", "Name", "CreatedOn"\nFROM "Contact"\nORDER BY "CreatedOn" DESC\nLIMIT 50';

/**
 * Run SQL against a Creatio environment (via clio + cliogate) and export the
 * result to CSV or Excel. Read-heavy by nature — the grid caps at 2,000 rows,
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

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvs(list);
      const active = list.find((e) => e.isActive) ?? list[0];
      if (active) setEnv(active.name);
    });
  }, []);

  const doRun = async () => {
    if (!env || !query.trim() || loading) return;
    setLoading(true);
    setError("");
    setNotice("");
    try {
      const res = await runSql(env, query);
      setResult(res);
      if (res.rowCount === 0) setNotice("Query ran — 0 rows returned.");
    } catch (e) {
      setResult(null);
      setError(String(e));
    } finally {
      setLoading(false);
    }
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
          <button className="primary" onClick={doRun} disabled={!env || loading}>
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
        writes the full result; the grid below shows up to 2,000 rows.
      </p>

      {notice && <p className="notice">{notice}</p>}
      {error && <p className="form-error">{error}</p>}

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
