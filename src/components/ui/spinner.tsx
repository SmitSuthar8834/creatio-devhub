import type { ComponentProps } from "react";
import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

/// A small inline spinner for buttons and text rows.
export function Spinner({ className, ...props }: ComponentProps<typeof Loader2>) {
  return <Loader2 className={cn("size-4 animate-spin", className)} aria-hidden="true" {...props} />;
}

/// A translucent overlay that covers its nearest positioned ancestor while
/// something loads. Place inside a `relative` container.
export function LoadingOverlay({ label }: { label?: string }) {
  return (
    <div
      className="absolute inset-0 z-20 flex flex-col items-center justify-center gap-3 rounded-lg bg-background/70 backdrop-blur-sm"
      role="status"
      aria-live="polite"
    >
      <Loader2 className="size-8 animate-spin text-primary" aria-hidden="true" />
      {label && <p className="text-sm font-medium text-muted-foreground">{label}</p>}
      <span className="sr-only">{label ?? "Loading"}</span>
    </div>
  );
}
