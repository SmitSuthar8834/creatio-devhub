import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { useErrorLog } from "../../lib/errorLog";

const DISMISS_MS = 8000;

/**
 * Headless driver that raises a toast whenever a new failure lands in the
 * app-wide error log — so an error on a background screen is noticeable without
 * having to be looking at it. The persisted backlog is not replayed: only
 * entries added during this session toast. Rendered through the shared sonner
 * <Toaster>.
 */
export default function ErrorToaster({ onShowErrors }: { onShowErrors: () => void }) {
  const entries = useErrorLog();
  // The newest id we've already toasted. Seeded on first run so existing
  // history stays quiet.
  const lastSeen = useRef<string | null | undefined>(undefined);

  useEffect(() => {
    const newest = entries[0];
    if (lastSeen.current === undefined) {
      lastSeen.current = newest?.id ?? null;
      return;
    }
    if (newest && newest.id !== lastSeen.current) {
      lastSeen.current = newest.id;
      const where = newest.context ? `${newest.source} · ${newest.context}` : newest.source;
      toast.error(`${where} failed`, {
        id: newest.id,
        description: newest.message.replace(/\s+/g, " ").slice(0, 160),
        action: { label: "View errors", onClick: onShowErrors },
        duration: DISMISS_MS,
      });
    }
  }, [entries, onShowErrors]);

  return null;
}
