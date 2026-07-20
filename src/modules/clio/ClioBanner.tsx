import { useCallback, useEffect, useState } from "react";
import { ClioStatus, getClioStatus, installOrUpdateClio, onJobUpdate } from "../../lib/ipc";

const DISMISS_KEY = "creatio-devhub.clio-update-dismissed.v1";
const CLIO_JOBS = ["install-clio", "update-clio", "repair-clio"];

type Mode = "install" | "update" | "repair";

/**
 * Header strip for the state of the clio CLI itself:
 *  - clio missing  → blocking prompt to install it (nothing in DevHub works without it)
 *  - clio damaged  → blocking prompt to repair (uninstall + reinstall)
 *  - newer build   → dismissible update prompt, with repair as a fallback
 * Each action runs `dotnet tool install/update/uninstall clio -g` as a streamed job.
 */
export default function ClioBanner({ onShowJobs }: { onShowJobs: () => void }) {
  const [status, setStatus] = useState<ClioStatus | null>(null);
  const [busy, setBusy] = useState<Mode | null>(null);
  const [dismissed, setDismissed] = useState<string>(() => localStorage.getItem(DISMISS_KEY) ?? "");

  const refresh = useCallback(() => {
    getClioStatus().then(setStatus).catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    refresh();
    // Re-check when a clio job settles so the banner clears (or stays) correctly.
    const un = onJobUpdate((job) => {
      if (!CLIO_JOBS.includes(job.kind)) return;
      if (job.status === "succeeded" || job.status === "failed" || job.status === "cancelled") {
        setBusy(null);
        refresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [refresh]);

  if (!status) return null;

  const run = async (mode: Mode) => {
    setBusy(mode);
    try {
      await installOrUpdateClio(mode);
      onShowJobs();
    } catch {
      setBusy(null);
    }
  };

  const label = (mode: Mode, idle: string, active: string) =>
    busy === mode ? active : idle;

  // 1. clio can't run at all.
  if (!status.installed) {
    const canInstall = !!status.dotnet;
    return (
      <div className="tool-banner bad">
        <span className="tb-dot" />
        <span className="tb-text">
          <strong>clio isn't installed.</strong> DevHub drives the clio CLI — install it to connect to your
          Creatio environments.
          {!canInstall && " The .NET SDK is required first (dotnet was not found on PATH)."}
        </span>
        {canInstall && (
          <button className="tb-btn" onClick={() => run("install")} disabled={busy !== null}>
            {label("install", "Install clio", "Installing…")}
          </button>
        )}
      </div>
    );
  }

  // 2. clio starts but can't load its own assemblies — an update won't fix this.
  if (status.broken) {
    return (
      <div className="tool-banner bad">
        <span className="tb-dot" />
        <span className="tb-text">
          <strong>clio's installation is damaged.</strong> A required assembly is missing, so clio commands
          will fail. Repairing reinstalls it cleanly.
        </span>
        <button className="tb-btn" onClick={() => run("repair")} disabled={busy !== null}>
          {label("repair", "Repair clio", "Repairing…")}
        </button>
      </div>
    );
  }

  // 3. A newer clio exists — dismissible per version, with repair as a fallback.
  if (status.updateAvailable && status.latest && status.latest !== dismissed) {
    return (
      <div className="tool-banner info">
        <span className="tb-dot" />
        <span className="tb-text">
          <strong>clio {status.latest} is available.</strong> You're on {status.version}.
        </span>
        <button className="tb-btn" onClick={() => run("update")} disabled={busy !== null}>
          {label("update", "Update clio", "Updating…")}
        </button>
        <button
          className="tb-btn ghost"
          title="Uninstall and reinstall clio — use this if updating keeps failing"
          onClick={() => run("repair")}
          disabled={busy !== null}
        >
          {label("repair", "Repair", "Repairing…")}
        </button>
        <button
          className="tb-x"
          title="Dismiss until the next version"
          onClick={() => {
            localStorage.setItem(DISMISS_KEY, status.latest as string);
            setDismissed(status.latest as string);
          }}
        >
          ✕
        </button>
      </div>
    );
  }

  return null;
}
