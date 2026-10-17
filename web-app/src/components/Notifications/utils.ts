import { appWindowName } from "@/libs/utils/appWindowName";
import type { InboxItem, NotificationLevel, SourceNode } from "@/types/notifications";

const APP_PATH = /^\/customer-apps\/([^/?#]+)\/([^/?#]+)(?:[/?#]|$)/;

/**
 * The window a notification's link opens in: the app's own named tab for a link
 * into a custom app (see `appWindowName` — an app never opens inside HQ), or
 * `null` for a path inside this SPA.
 */
export function appWindowFor(href: string): string | null {
  const match = APP_PATH.exec(href);
  return match ? appWindowName(match[1], match[2]) : null;
}

/** The implicit category of a kind that names none. Not worth a label on a row. */
export const GENERAL_CATEGORY = "general";

/**
 * Where a notification came from, for its row: "Store Ops · Food safety". Just
 * the source for the implicit General category, or from a server too old to
 * send a category.
 */
export function sourceLine(item: InboxItem): string {
  const source = item.app?.name ?? "Oxygen";
  const category = item.category;
  if (!category || category.path === GENERAL_CATEGORY) return source;
  const leaf = category.labels.at(-1);
  return leaf ? `${source} · ${leaf}` : source;
}

/** The levels that earn a row an accent, and what a screen reader hears for it. */
export const LEVEL_ACCENT: Partial<
  Record<NotificationLevel, { border: string; text: string; label: string }>
> = {
  critical: { border: "border-l-destructive", text: "text-destructive", label: "Critical" },
  time_sensitive: { border: "border-l-warning", text: "text-warning", label: "Time sensitive" }
};

/** Oxygen above the apps, the apps in the order the server sent them. */
export function oxygenFirst<T extends Pick<SourceNode, "source">>(sources: T[]): T[] {
  return [
    ...sources.filter((s) => s.source === "oxygen"),
    ...sources.filter((s) => s.source !== "oxygen")
  ];
}
