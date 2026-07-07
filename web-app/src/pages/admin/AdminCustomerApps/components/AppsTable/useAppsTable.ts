import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import type { CustomerApp } from "@/types/apps";

/** Card grid vs. dense list — the two ways to look at the registry. */
export type ViewMode = "gallery" | "list";
/** How the table buckets rows. `org` matches the historical default. */
export type GroupBy = "none" | "org" | "status" | "source";
export type SortKey = "name" | "org" | "source" | "status" | "active" | "updated";
export type SortDir = "asc" | "desc";
export type StatusFilter = "all" | "live" | "draft";
export type SourceFilter = "all" | "v0" | "local" | "s3";

export interface AppsTableState {
  view: ViewMode;
  q: string;
  source: SourceFilter;
  status: StatusFilter;
  group: GroupBy;
  sortKey: SortKey;
  sortDir: SortDir;
}

export interface AppGroup {
  /** Stable key (collapse state, React key). Empty group uses `__all__`. */
  key: string;
  /** Header label; empty when `group === "none"` (no header rendered). */
  label: string;
  items: CustomerApp[];
}

export interface AppsTableModel {
  groups: AppGroup[];
  /** Visual top-to-bottom row order — drives select-all + shift-range. */
  flatIds: string[];
  filteredCount: number;
  totalCount: number;
}

export const DEFAULT_TABLE_STATE: AppsTableState = {
  view: "gallery",
  q: "",
  source: "all",
  status: "all",
  group: "org",
  sortKey: "updated",
  sortDir: "desc"
};

/** Sort keys that read most-useful newest/live-first, so a fresh click on
 *  them defaults to descending; the rest default to ascending (A→Z). */
const DESC_FIRST: ReadonlySet<SortKey> = new Set(["status", "active", "updated"]);

export const defaultDirFor = (key: SortKey): SortDir => (DESC_FIRST.has(key) ? "desc" : "asc");

/** Tri-state select for a group of rows: none → false, all → true, else
 *  "indeterminate". Shared by both views so their group headers can't drift. */
export const groupCheckState = (
  ids: string[],
  isSelected: (id: string) => boolean
): boolean | "indeterminate" => {
  const n = ids.filter(isSelected).length;
  return n === 0 ? false : n === ids.length ? true : "indeterminate";
};

const SOURCE_LABEL: Record<CustomerApp["source_type"], string> = {
  v0: "v0",
  local: "Local",
  s3: "S3"
};

/**
 * The one pure transform behind the whole table: filter → sort → group. Kept
 * side-effect-free (no hooks, no `Date.now` beyond the recency helpers) so it
 * unit-tests directly. `flatIds` is the concatenation of every group's items
 * in display order, so shift-click ranges and select-all stay honest whatever
 * the grouping.
 */
export function buildAppsTableModel(apps: CustomerApp[], state: AppsTableState): AppsTableModel {
  const needle = state.q.trim().toLowerCase();
  const filtered = apps.filter((a) => {
    if (state.source !== "all" && a.source_type !== state.source) return false;
    if (state.status === "live" && !a.published_at) return false;
    if (state.status === "draft" && a.published_at) return false;
    if (!needle) return true;
    return (
      a.name.toLowerCase().includes(needle) ||
      a.slug.toLowerCase().includes(needle) ||
      a.org_slug.toLowerCase().includes(needle) ||
      a.project_id.toLowerCase().includes(needle)
    );
  });

  const sign = state.sortDir === "asc" ? 1 : -1;
  const sorted = [...filtered].sort((a, b) => sign * compareBy(state.sortKey, a, b));

  const groups =
    state.group === "none"
      ? [{ key: "__all__", label: "", items: sorted }]
      : partition(sorted, state.group);

  return {
    groups,
    flatIds: groups.flatMap((g) => g.items.map((a) => a.id)),
    filteredCount: filtered.length,
    totalCount: apps.length
  };
}

function compareBy(key: SortKey, a: CustomerApp, b: CustomerApp): number {
  switch (key) {
    case "name":
      return a.name.localeCompare(b.name);
    case "org":
      return a.org_slug.localeCompare(b.org_slug) || a.name.localeCompare(b.name);
    case "source":
      return a.source_type.localeCompare(b.source_type) || a.name.localeCompare(b.name);
    case "status":
      return statusRank(a) - statusRank(b) || a.name.localeCompare(b.name);
    case "active":
      return activeScore(a) - activeScore(b);
    case "updated":
      return recencyScore(a) - recencyScore(b);
  }
}

function partition(sorted: CustomerApp[], group: Exclude<GroupBy, "none">): AppGroup[] {
  // Map preserves insertion order, so groups land ordered by their
  // top (already-sorted) row — the group holding the #1 row comes first.
  const map = new Map<string, CustomerApp[]>();
  for (const a of sorted) {
    const key = groupKey(a, group);
    const list = map.get(key) ?? [];
    list.push(a);
    map.set(key, list);
  }
  return [...map.entries()].map(([key, items]) => ({
    key,
    label: groupLabel(key, group),
    items
  }));
}

function groupKey(a: CustomerApp, group: Exclude<GroupBy, "none">): string {
  if (group === "org") return a.org_slug;
  if (group === "status") return a.published_at ? "live" : "draft";
  return a.source_type;
}

function groupLabel(key: string, group: Exclude<GroupBy, "none">): string {
  if (group === "org") return key;
  if (group === "status") return key === "live" ? "Live" : "Draft";
  return SOURCE_LABEL[key as CustomerApp["source_type"]] ?? key.toUpperCase();
}

const statusRank = (a: CustomerApp): number => (a.published_at ? 1 : 0);

/** Millisecond epoch of `updated_at` (falls back to `created_at`); 0 on a
 *  bad/absent value so a broken row sinks instead of throwing. */
export function recencyScore(a: CustomerApp): number {
  return epoch(a.updated_at ?? a.created_at);
}

/** "Is this being used?" score: last browser view, else last registry sync. */
export function activeScore(a: CustomerApp): number {
  return epoch(a.last_active_at ?? a.last_synced_at ?? null);
}

function epoch(iso: string | null | undefined): number {
  if (!iso) return 0;
  const t = new Date(iso).getTime();
  return Number.isFinite(t) ? t : 0;
}

/** Compact "2m / 3h / 4d" stamp. `—` for an absent/unparseable value. */
export function formatRelativeTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const sec = Math.round((Date.now() - then) / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d`;
  return `${Math.round(day / 30)}mo`;
}

// ── URL-synced table state ────────────────────────────────────────────────

const oneOf = <T extends string>(v: string | null, allowed: readonly T[], fallback: T): T =>
  allowed.includes(v as T) ? (v as T) : fallback;

function parseState(p: URLSearchParams): AppsTableState {
  return {
    view: oneOf(p.get("vw"), ["gallery", "list"], "gallery"),
    q: p.get("q") ?? "",
    source: oneOf(p.get("source"), ["all", "v0", "local", "s3"], "all"),
    status: oneOf(p.get("status"), ["all", "live", "draft"], "all"),
    group: oneOf(p.get("group"), ["none", "org", "status", "source"], "org"),
    sortKey: oneOf(
      p.get("sort"),
      ["name", "org", "source", "status", "active", "updated"],
      "updated"
    ),
    sortDir: oneOf(p.get("dir"), ["asc", "desc"], "desc")
  };
}

/** Write only non-default values so the URL stays clean and shareable. */
function writeState(p: URLSearchParams, s: AppsTableState) {
  const set = (k: string, v: string, def: string) => (v === def ? p.delete(k) : p.set(k, v));
  set("vw", s.view, "gallery");
  set("q", s.q, "");
  set("source", s.source, "all");
  set("status", s.status, "all");
  set("group", s.group, "org");
  set("sort", s.sortKey, "updated");
  set("dir", s.sortDir, "desc");
}

/**
 * Table filter/sort/group state, persisted in the URL query so a filtered view
 * is shareable and survives the back button. Only touches its own keys — the
 * `view` tab and selected-app path segments are left intact.
 */
export function useAppsTableState(): [AppsTableState, (patch: Partial<AppsTableState>) => void] {
  const [params, setParams] = useSearchParams();
  const state = useMemo(() => parseState(params), [params]);
  const setState = useCallback(
    (patch: Partial<AppsTableState>) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          writeState(next, { ...parseState(prev), ...patch });
          return next;
        },
        { replace: true }
      );
    },
    [setParams]
  );
  return [state, setState];
}
