import { useEffect, useState } from "react";

/// Appearance mode. "system" follows the OS; the other two pin it.
export type ThemeMode = "system" | "light" | "dark";

const KEY = "devhub.theme";
const media = () => window.matchMedia("(prefers-color-scheme: dark)");

export function getThemeMode(): ThemeMode {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
  } catch {
    /* storage unavailable — treat as unset */
  }
  return "system";
}

/// Which theme is actually showing, resolving "system" against the OS.
export function resolvedTheme(mode: ThemeMode): "light" | "dark" {
  if (mode !== "system") return mode;
  return media().matches ? "dark" : "light";
}

/// Tailwind's dark: variant keys off a .dark class on <html>.
export function applyThemeMode(mode: ThemeMode): void {
  document.documentElement.classList.toggle("dark", resolvedTheme(mode) === "dark");
}

export function setThemeMode(mode: ThemeMode): void {
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    /* not fatal: the mode still applies for this session */
  }
  applyThemeMode(mode);
  window.dispatchEvent(new CustomEvent<ThemeMode>("devhub-theme", { detail: mode }));
}

/// Current mode + resolved theme, staying in sync with Settings changes and,
/// in system mode, with the OS switching. Used by the Settings radio group and
/// the sonner toaster (which needs the resolved theme, not the mode).
export function useTheme(): { mode: ThemeMode; resolved: "light" | "dark" } {
  const [mode, setMode] = useState<ThemeMode>(getThemeMode);
  const [resolved, setResolved] = useState<"light" | "dark">(() => resolvedTheme(getThemeMode()));

  useEffect(() => {
    const sync = () => {
      const current = getThemeMode();
      setMode(current);
      setResolved(resolvedTheme(current));
      applyThemeMode(current);
    };
    const onModeChange = () => sync();
    const mq = media();
    window.addEventListener("devhub-theme", onModeChange);
    mq.addEventListener("change", onModeChange);
    return () => {
      window.removeEventListener("devhub-theme", onModeChange);
      mq.removeEventListener("change", onModeChange);
    };
  }, []);

  return { mode, resolved };
}
