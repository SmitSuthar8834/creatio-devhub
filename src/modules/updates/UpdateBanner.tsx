import { useCallback, useEffect, useRef, useState } from "react";
import { Sparkles, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { checkForAppUpdate, installAppUpdate, Update } from "../../lib/appUpdate";

const DISMISS_KEY = "creatio-devhub.app-update-dismissed.v1";
/** Let the window paint and the catalog prefetch start before hitting the network. */
const FIRST_CHECK_MS = 4000;
/** DevHub stays open for days at a time, so a launch-only check would miss releases. */
const RECHECK_MS = 6 * 60 * 60 * 1000;

/**
 * Header strip announcing a new DevHub release, so an update no longer waits for
 * someone to visit Settings and press "Check for updates". It checks shortly
 * after launch and every six hours the app stays open.
 *
 * A failed check is deliberately silent: DevHub is useful offline and on VPNs
 * that block github.com, and an unreachable update feed is not the user's
 * problem to solve mid-task. Settings → DevHub updates still reports the reason
 * on demand.
 *
 * Dismissal is stored per version, matching ClioBanner: hiding v0.5.0 must not
 * hide v0.6.0.
 */
export default function UpdateBanner({ onShowSettings }: { onShowSettings: () => void }) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(DISMISS_KEY) ?? "");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);
  /** An install is in flight; a scheduled check must not swap the update under it. */
  const installing = useRef(false);

  const look = useCallback(async () => {
    if (installing.current) return;
    try {
      setUpdate(await checkForAppUpdate());
    } catch {
      // Offline, blocked, or the feed is down — stay quiet and try again later.
    }
  }, []);

  useEffect(() => {
    const first = setTimeout(look, FIRST_CHECK_MS);
    const repeat = setInterval(look, RECHECK_MS);
    return () => {
      clearTimeout(first);
      clearInterval(repeat);
    };
  }, [look]);

  if (!update || update.version === dismissed) return null;

  const install = async () => {
    installing.current = true;
    setBusy(true);
    setFailed(false);
    try {
      await installAppUpdate(update, setProgress);
    } catch {
      // The restart never happened — say so and leave the banner in place.
      installing.current = false;
      setBusy(false);
      setProgress(null);
      setFailed(true);
    }
  };

  return (
    <div className="flex shrink-0 flex-col gap-2 border-b bg-primary/10 px-4 py-2.5 text-sm">
      <div className="flex items-center gap-3">
        <Sparkles className="size-4 shrink-0 text-primary" aria-hidden="true" />
        <p className="flex-1">
          <strong className="font-semibold">DevHub {update.version} is available.</strong>{" "}
          {failed
            ? "The update could not be installed — open Settings for the details."
            : busy
              ? "Downloading — DevHub restarts on its own when it finishes."
              : "Installing takes a few seconds and restarts the app."}
        </p>
        <Button size="sm" onClick={install} disabled={busy}>
          {busy ? "Installing…" : `Install and restart`}
        </Button>
        <Button size="sm" variant="outline" onClick={onShowSettings} disabled={busy}>
          What's new
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="size-7"
          title="Dismiss until the next version"
          disabled={busy}
          onClick={() => {
            localStorage.setItem(DISMISS_KEY, update.version);
            setDismissed(update.version);
          }}
        >
          <X aria-hidden="true" />
          <span className="sr-only">Dismiss</span>
        </Button>
      </div>
      {progress !== null && <Progress value={progress} />}
    </div>
  );
}
