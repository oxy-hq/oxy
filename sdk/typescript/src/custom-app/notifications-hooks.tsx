// `useNotifications` — the person's inbox, read from inside a custom app.
//
// Polls the cheap unread count (Oxy's own database, never the org's store)
// every 30 s while the tab is visible, and fetches the list only when asked —
// the bell asks when it opens. Never throws: a server without the routes, or a
// page nobody is signed in to, leaves `available` false and the bell renders
// nothing; any other failure is reported on `error`.

import * as React from "react";
import { readAppEnvironment } from "./inject";
import {
  createInboxClient,
  type InboxClient,
  type InboxFilter,
  type InboxOutcome,
  isFiltered,
  matchesFilter,
  type NotificationItem,
  type NotificationsScope,
  serverKnowsCategories,
  serverScopesToApp,
  WRITE_REFUSED
} from "./notifications";
import { useOxyApp, useResolvedManifest } from "./react";

export interface UseNotificationsOptions {
  /**
   * `"app"` (default): this app's notifications only — what it sent with
   * `ctx.notify`. `"all"`: the person's whole inbox, other apps and Oxygen's
   * own notices included.
   */
  scope?: NotificationsScope;
  /**
   * Only this category of this app's catalogue (`oxy-app.json`
   * `notifications.categories`) and the categories under it:
   * `"operations"` covers `"operations/food-safety"`. Names this app, so it
   * needs the app id oxy injects (not available in `pnpm dev`).
   */
  category?: string;
  /** Only this kind (`"temp_alarm"`). */
  kind?: string;
  /** Only this thread — an item's `thread`. */
  thread?: string;
  /** How often to poll the unread count, in ms. Default 30 000; 0 polls once. */
  pollInterval?: number;
  /** How many items `refresh()` fetches. Default 20, at most 100. */
  limit?: number;
  /** Set false to make no requests at all. */
  enabled?: boolean;
}

export interface UseNotificationsResult {
  /**
   * Newest first, read and unread, narrowed by `category` / `kind` / `thread`.
   * Empty until `refresh()` has run.
   */
  items: NotificationItem[];
  /**
   * Unread count, kept current by polling — narrowed by `category` / `kind` /
   * `thread` like `items` (a server older than categories counts the scope).
   */
  unread: number;
  /** True while `refresh()` is fetching the list. */
  isLoading: boolean;
  /** The last failure, or null. Never thrown. */
  error: Error | null;
  /**
   * False until the inbox has answered once, and for good when it cannot: a
   * server without the routes, nobody signed in, or a page with no app id (a
   * `pnpm dev` server) under `scope: "app"`. Render nothing while false.
   */
  available: boolean;
  /**
   * True on a staging or `dev-*` app host, which may read the inbox but not
   * change read state (read state carries no environment, so it would change
   * production's). `markRead` and `markAllRead` do nothing there.
   */
  readOnly: boolean;
  /**
   * Mark one read. Optimistic; a failure lands on `error` and the state
   * re-syncs. A 403 (a host that refuses writes) re-syncs without an error.
   */
  markRead: (id: string) => Promise<void>;
  /**
   * Mark every item in this scope read — only what `category` / `kind` /
   * `thread` cover, when set. Same failure rules as `markRead`.
   */
  markAllRead: () => Promise<void>;
  /** Fetch the list (and the count with it). */
  refresh: () => Promise<void>;
}

const DEFAULT_POLL_MS = 30_000;
const DEFAULT_LIMIT = 20;

type Status = "unknown" | "available" | "unavailable";

/** `scope: "app"` and a `category` need to know which app this is; `pnpm dev` does not say. */
const NO_APP_ID = new Error(
  'useNotifications: this page has no app id (window.__OXY_APP__.appId), so scope "app" ' +
    "(or a category, which names one of this app's) cannot be honored — oxy sets it when it " +
    'serves the app. Use scope: "all", without a category, in pnpm dev.'
);

/** A server that ignored `app_id`: its "mark all read" would clear every app's. */
const NOT_SCOPED = new Error(
  "useNotifications: this server cannot narrow the inbox to one app yet, so the app's " +
    'inbox is hidden. Use scope: "all", or upgrade the server.'
);

function nowIso(): string {
  return new Date().toISOString();
}

function markAllInList(items: NotificationItem[]): NotificationItem[] {
  const at = nowIso();
  return items.map((i) => (i.read_at === null ? { ...i, read_at: at } : i));
}

/**
 * The person's notifications, from inside a custom app. Must be called inside
 * `<OxyAppProvider>`.
 *
 * ```tsx
 * const inbox = useNotifications();
 * if (!inbox.available) return null;
 * return <button onClick={() => void inbox.refresh()}>Inbox ({inbox.unread})</button>;
 * ```
 */
export function useNotifications(opts: UseNotificationsOptions = {}): UseNotificationsResult {
  const { fetcher } = useOxyApp();
  const { appId, apiBaseUrl } = useResolvedManifest();
  const scope = opts.scope ?? "app";
  const enabled = opts.enabled !== false;
  const pollInterval = Math.max(0, opts.pollInterval ?? DEFAULT_POLL_MS);
  const limit = Math.min(100, Math.max(1, opts.limit ?? DEFAULT_LIMIT));
  const { category, kind, thread } = opts;
  // A category is a path in this app's tree, so it names this app as `source`.
  const filter = React.useMemo<InboxFilter>(
    () => ({ category, kind, thread, source: category ? appId : undefined }),
    [category, kind, thread, appId]
  );
  const missingApp = (scope === "app" || Boolean(category)) && !appId;
  // Absent on a server older than app environments, which is production.
  const environment = readAppEnvironment();
  const readOnly = environment !== undefined && environment !== "production";

  const client = React.useMemo(
    () => createInboxClient(fetcher, apiBaseUrl, scope === "app" ? appId : undefined),
    [fetcher, apiBaseUrl, scope, appId]
  );
  const reads = React.useMemo<Reads>(() => ({ client, filter }), [client, filter]);
  const store = useInboxState(missingApp, reads);
  const refresh = useRefresh(reads, store, { scope, limit, missingApp });
  const pollUnread = usePollUnread(reads, store, refresh);
  usePolling(enabled && !missingApp && store.status !== "unavailable", pollInterval, pollUnread);
  const { markRead, markAllRead } = useMarking(reads, store, {
    scope,
    readOnly,
    refresh,
    pollUnread
  });

  return {
    items: store.items,
    unread: store.unread,
    isLoading: store.isLoading,
    error: store.error,
    available: store.status === "available",
    readOnly,
    markRead,
    markAllRead,
    refresh
  };
}

// ── State ───────────────────────────────────────────────────────────────────

/** What the list reads: the client (scope, app) and the filter. A new one starts over. */
interface Reads {
  client: InboxClient;
  filter: InboxFilter;
}

interface InboxState {
  items: NotificationItem[];
  unread: number;
  isLoading: boolean;
  error: Error | null;
  status: Status;
  set: React.Dispatch<React.SetStateAction<Snapshot>>;
  /** Mutable facts the callbacks read without re-subscribing. */
  live: React.MutableRefObject<{
    mounted: boolean;
    items: NotificationItem[];
    unread: number;
    listed: boolean;
    scoped: boolean | null;
    /** Did the server show it filters by category, kind and thread? */
    filters: boolean | null;
  }>;
}

type Snapshot = Omit<InboxState, "set" | "live">;

function initialSnapshot(missingApp: boolean): Snapshot {
  return {
    items: [],
    unread: 0,
    isLoading: false,
    error: missingApp ? NO_APP_ID : null,
    status: missingApp ? "unavailable" : "unknown"
  };
}

/** The hook's state, started over when what it reads changes (scope, app, filter). */
function useInboxState(missingApp: boolean, reads: Reads): InboxState {
  const [state, set] = React.useState<Snapshot>(() => initialSnapshot(missingApp));
  const live = React.useRef({
    mounted: true,
    items: state.items,
    unread: state.unread,
    listed: false,
    scoped: null as boolean | null,
    filters: null as boolean | null
  });
  live.current.items = state.items;
  live.current.unread = state.unread;
  React.useEffect(() => {
    live.current.mounted = true;
    return () => {
      live.current.mounted = false;
    };
  }, []);
  const readFrom = React.useRef(reads);
  React.useEffect(() => {
    if (readFrom.current === reads) return;
    readFrom.current = reads;
    live.current.listed = false;
    live.current.scoped = null;
    live.current.filters = null;
    set(initialSnapshot(missingApp));
  }, [reads, missingApp]);
  return { ...state, set, live };
}

/** Record a failed request: `unavailable` turns the inbox off for good. */
function fail(
  set: InboxState["set"],
  outcome: Extract<InboxOutcome<unknown>, { ok: false }>
): void {
  set((s) => ({
    ...s,
    isLoading: false,
    error: outcome.error,
    status: outcome.unavailable ? "unavailable" : s.status
  }));
}

// ── Reads ───────────────────────────────────────────────────────────────────

function useRefresh(
  { client, filter }: Reads,
  { set, live }: InboxState,
  opts: { scope: NotificationsScope; limit: number; missingApp: boolean }
): () => Promise<void> {
  const { scope, limit, missingApp } = opts;
  return React.useCallback(async () => {
    if (missingApp) return;
    set((s) => ({ ...s, isLoading: true }));
    const r = await client.list(limit, filter);
    if (!live.current.mounted) return;
    if (!r.ok) return fail(set, r);
    const scoped = serverScopesToApp(r.value.items);
    if (scope === "app" && scoped === false) {
      live.current.scoped = false;
      set((s) => ({ ...s, isLoading: false, error: NOT_SCOPED, status: "unavailable" }));
      return;
    }
    if (scoped) live.current.scoped = true;
    const filters = serverKnowsCategories(r.value.items);
    if (filters !== null) live.current.filters = filters;
    live.current.listed = true;
    // Again here: a server older than the filter returns everything. Kept on
    // `live` at once, for a caller that reads it right after this resolves.
    const items = r.value.items.filter((item) => matchesFilter(item, filter));
    live.current.items = items;
    set({ items, unread: r.value.unread, isLoading: false, error: null, status: "available" });
  }, [client, filter, limit, scope, missingApp, set, live]);
}

function usePollUnread(
  { client, filter }: Reads,
  { set, live }: InboxState,
  refresh: () => Promise<void>
): () => Promise<void> {
  return React.useCallback(async () => {
    const r = await client.unread(filter);
    if (!live.current.mounted) return;
    if (!r.ok) return fail(set, r);
    const changed = r.value !== live.current.unread;
    set((s) => ({ ...s, unread: r.value, error: null, status: "available" }));
    // The list is open somewhere and something arrived (or was read elsewhere).
    if (changed && live.current.listed) void refresh();
  }, [client, filter, refresh, set, live]);
}

/** Poll now, every `interval` ms while the tab is visible, and on coming back to it. */
function usePolling(active: boolean, interval: number, poll: () => Promise<void>): void {
  React.useEffect(() => {
    if (!active || typeof document === "undefined") return;
    const tick = () => {
      if (!document.hidden) void poll();
    };
    tick();
    const id = interval > 0 ? setInterval(tick, interval) : null;
    document.addEventListener("visibilitychange", tick);
    return () => {
      if (id !== null) clearInterval(id);
      document.removeEventListener("visibilitychange", tick);
    };
  }, [active, interval, poll]);
}

// ── Writes ──────────────────────────────────────────────────────────────────

interface MarkingDeps {
  scope: NotificationsScope;
  readOnly: boolean;
  refresh: () => Promise<void>;
  pollUnread: () => Promise<void>;
}

function useMarking(
  { client, filter }: Reads,
  { set, live }: InboxState,
  { scope, readOnly, refresh, pollUnread }: MarkingDeps
): Pick<UseNotificationsResult, "markRead" | "markAllRead"> {
  /**
   * After a write: re-sync with the server — the open list if there is one,
   * else the count — then report a failure (after, so the re-sync's success
   * does not wipe it). A 403 is a host that refuses writes, not a failure to
   * show; the bell stays up.
   */
  const settle = React.useCallback(
    async (r: InboxOutcome<void>, always: boolean) => {
      if (!live.current.mounted || (r.ok && !always)) return;
      await (live.current.listed ? refresh() : pollUnread());
      if (r.ok || r.status === WRITE_REFUSED || !live.current.mounted) return;
      set((s) => ({ ...s, error: r.error }));
    },
    [set, live, refresh, pollUnread]
  );

  const markRead = React.useCallback(
    async (id: string) => {
      if (readOnly) return;
      const known = live.current.items.find((i) => i.id === id);
      if (known && known.read_at !== null) return;
      const at = nowIso();
      set((s) => ({
        ...s,
        items: s.items.map((i) => (i.id === id ? { ...i, read_at: at } : i)),
        unread: known ? Math.max(0, s.unread - 1) : s.unread
      }));
      // An item that was not in the list was not counted down: re-sync then too.
      await settle(await client.markRead(id), !known);
    },
    [client, set, live, readOnly, settle]
  );

  const markFiltered = useMarkFiltered({ client, filter }, { set, live }, refresh, settle);

  const markAllRead = React.useCallback(async () => {
    if (readOnly) return;
    if (isFiltered(filter)) return markFiltered();
    // Never "read-all" under an app scope the server may not honor: learn first.
    // (A function, so the check after the await reads the ref afresh.)
    const confirmed = () => live.current.scoped === true;
    if (scope === "app" && !confirmed()) {
      await refresh();
      if (!confirmed()) return;
    }
    set((s) => ({ ...s, items: markAllInList(s.items), unread: 0 }));
    await settle(await client.markAllRead(), false);
  }, [client, filter, scope, set, live, readOnly, refresh, settle, markFiltered]);

  return { markRead, markAllRead };
}

/**
 * "Mark all read" under a category, kind or thread. A server that showed it
 * filters gets one `read-all` with the filter; one that has not (older, or an
 * empty list to judge by) gets each listed unread item marked on its own —
 * its `read-all` would ignore the filter and clear the whole scope.
 */
function useMarkFiltered(
  { client, filter }: Reads,
  { set, live }: Pick<InboxState, "set" | "live">,
  refresh: () => Promise<void>,
  settle: (r: InboxOutcome<void>, always: boolean) => Promise<void>
): () => Promise<void> {
  return React.useCallback(async () => {
    if (live.current.filters !== true) await refresh();
    const unread = live.current.items.filter((i) => i.read_at === null);
    if (unread.length === 0) return;
    set((s) => ({
      ...s,
      items: markAllInList(s.items),
      unread: Math.max(0, s.unread - unread.length)
    }));
    if (live.current.filters === true) {
      await settle(await client.markAllRead(filter), true);
      return;
    }
    const outcomes = await Promise.all(unread.map((i) => client.markRead(i.id)));
    await settle(
      outcomes.find((r) => !r.ok) ?? outcomes[0] ?? { ok: true, value: undefined },
      true
    );
  }, [client, filter, set, live, refresh, settle]);
}
