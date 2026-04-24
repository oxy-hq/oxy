import type { AppItem } from "@/types/app";

/** Humanize a filename-derived app name for display (e.g. `raw_orders` →
 *  `Raw Orders`, `LINEITEM` → `Lineitem`). Used as a fallback when the app
 *  does not declare a `title:` field. */
export function humanizeAppName(name: string): string {
  return name
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0) + w.slice(1).toLowerCase())
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** Label rendered in sidebar + completion card for a workspace app.
 *  Prefers the LLM-authored `title:` field (inferred from the data), falls
 *  back to a humanized form of the filename. Kept in one place so sidebar
 *  and onboarding completion stay in sync. */
export function appDisplayLabel(app: Pick<AppItem, "name" | "title">): string {
  const title = app.title?.trim();
  if (title) return title;
  return humanizeAppName(app.name);
}
