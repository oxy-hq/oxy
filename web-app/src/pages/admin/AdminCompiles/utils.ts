/** Slice a possibly-undefined string. Lets rows render without crashing
 *  on a malformed row from a misconfigured deployment. */
export function safeSlice(value: string | null | undefined, end: number): string {
  if (typeof value !== "string") return "—";
  return value.slice(0, end);
}

export function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const diff = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

/** Tailwind classes for a compile status accent, reusing the page's
 *  existing token convention (emerald=ready, destructive=failed,
 *  amber=compiling). */
export function statusAccent(status: string | null | undefined): string {
  switch (status) {
    case "ready":
      return "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
    case "failed":
      return "border-destructive/40 bg-destructive/10 text-destructive";
    case "compiling":
      return "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300";
    default:
      return "border-border/60 bg-muted/40 text-muted-foreground";
  }
}
