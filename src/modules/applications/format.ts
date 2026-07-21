/**
 * Formatting shared by the Applications screen and its details dialog.
 *
 * Kept out of the component modules on purpose: React Fast Refresh cannot
 * hot-patch a file that exports anything other than components, so a helper
 * living in `ApplicationsPage.tsx` forced a full invalidate on every edit.
 */

/** Creatio returns dates as text; show them as dates when they parse. */
export function shortDate(value: string): string {
  if (!value) return "";
  // Creatio returns dd-MM-yyyy HH:mm:ss, which Date cannot parse on its own.
  const parts = value.match(/^(\d{2})-(\d{2})-(\d{4})/);
  const parsed = parts
    ? new Date(Number(parts[3]), Number(parts[2]) - 1, Number(parts[1]))
    : new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleDateString();
}
