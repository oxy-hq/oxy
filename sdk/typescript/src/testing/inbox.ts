// A fake inbox for an app's UI tests: a `fetch` that answers the four
// `/api/notifications` routes the way the server does, from items the test
// holds — so `useNotifications()` and `<NotificationsBell />` can be rendered
// with no server, and their reads and writes checked afterwards.
//
//   const inbox = createTestInbox({ appId: "app-1" });
//   inbox.receive({ kind: "booking_created", title: "New booking", link: "/bookings/1" });
//   render(<OxyAppProvider fetcher={inbox.fetcher}><NotificationsBell /></OxyAppProvider>);
//
// `receive` takes the same payload a function hands `ctx.notify`, so what a
// function test sent (`state.notifications[i].input`) is what the bell shows —
// labelled, grouped and ranked by the app's catalogue when `manifest` is given.
// Pure TypeScript, no React, like the rest of this entry.

import { createHash } from "node:crypto";
import type { NotifyInput } from "../custom-app/function-context";
import {
  GENERAL_CATEGORY,
  humanizeNotificationName,
  resolveNotificationKind
} from "../custom-app/notification-catalog";
import type { NotificationApp, NotificationItem } from "../custom-app/notifications";

export interface TestInboxOptions {
  /** The app `app_id` narrows to, and the default `app` of a new item. Default `"app-test"`. */
  appId?: string;
  /** The default app's slug, and the org slug in its `href`. */
  appSlug?: string;
  orgSlug?: string;
  /**
   * The app's `oxy-app.json`: `receive` gives an item the category, label,
   * level and actions its kind has in the `notifications` catalogue, as the
   * server does. Without it, every kind gets the defaults.
   */
  manifest?: unknown;
  /** Items to start with; each is completed as `push` completes it. */
  items?: Partial<NotificationItem>[];
  /**
   * Answer every inbox route with this status instead: 404 is a server
   * without the routes, 401 a page nobody is signed in to.
   */
  status?: number;
  /**
   * Answer the two writes (`/{id}/read`, `/read-all`) with this status
   * instead, changing nothing: 403 is a staging or `dev-*` app host, which
   * refuses them (`EnvironmentRefused`) while reads work.
   */
  writeStatus?: number;
  /**
   * Behave as a server older than the `app_id` filter: ignore it and every
   * other filter, and send items without `link` or any catalogue field
   * (`category`, `kind_label`, `level`, `thread`, `actions`, `url`).
   */
  legacy?: boolean;
  /** Answers every request outside `/api/notifications`. Default: a 404. */
  fallback?: typeof fetch;
}

/** One request the inbox answered. */
export interface TestInboxRequest {
  method: string;
  /** Path without the query, e.g. `/api/notifications/read-all`. */
  path: string;
  query: Record<string, string>;
}

export interface TestInbox {
  /** Pass to `<OxyAppProvider fetcher={…}>`. */
  readonly fetcher: typeof fetch;
  /** The inbox, newest first; reads and writes act on it. */
  readonly items: NotificationItem[];
  /** Every request the inbox answered, in order. */
  readonly requests: TestInboxRequest[];
  /** Add an item, completed with defaults; returns it. */
  push(item?: Partial<NotificationItem>): NotificationItem;
  /**
   * Add what a function sent with `ctx.notify`, as its recipient would see it
   * — a `ctx.notify` payload, or a test context's `state.notifications[i].input`.
   */
  receive(input: NotifyPayload): NotificationItem;
  /** Unread count, for one app or the whole inbox. */
  unread(appId?: string): number;
}

/** The part of a `ctx.notify` payload the inbox shows. */
export type NotifyPayload =
  | Pick<NotifyInput, "kind" | "title" | "body" | "link" | "thread">
  | Readonly<Record<string, unknown>>;

const MAX_PAGE = 100;

/** The fields a server older than categories does not send. */
const CATALOGUE_FIELDS = ["category", "kind_label", "level", "thread", "actions", "url"] as const;

const text = (v: unknown): string | null => (typeof v === "string" ? v : null);

/**
 * What an item carries for a `thread`: the server stores and sends only a hash
 * of the app's text — SHA-256, hex, its first 32 characters.
 */
export function threadKey(thread: string): string {
  return createHash("sha256").update(thread).digest("hex").slice(0, 32);
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" }
  });
}

function parse(input: RequestInfo | URL): { path: string; query: Record<string, string> } {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  const url = new URL(raw, "http://inbox.test");
  const query: Record<string, string> = {};
  url.searchParams.forEach((v, k) => {
    query[k] = v;
  });
  return { path: url.pathname, query };
}

export function createTestInbox(options: TestInboxOptions = {}): TestInbox {
  const appId = options.appId ?? "app-test";
  const app: NotificationApp = { id: appId, slug: options.appSlug ?? "test-app", name: "Test app" };
  const orgSlug = options.orgSlug ?? "test-org";
  const items: NotificationItem[] = [];
  const requests: TestInboxRequest[] = [];
  let seq = 0;

  function push(over: Partial<NotificationItem> = {}): NotificationItem {
    seq += 1;
    const itemApp = over.app === undefined ? app : over.app;
    const link = over.link === undefined ? null : over.link;
    const href = itemApp ? `/customer-apps/${orgSlug}/${itemApp.slug}${link ?? "/"}` : null;
    const kind = over.kind ?? "test";
    const item: NotificationItem = {
      id: `notification-${seq}`,
      org_id: "org-test",
      kind,
      created_at: new Date(Date.UTC(2026, 0, 1, 0, 0, seq)).toISOString(),
      read_at: null,
      title: `Notification ${seq}`,
      body: null,
      href,
      link,
      app: itemApp,
      category: { path: GENERAL_CATEGORY, labels: ["General"] },
      kind_label: humanizeNotificationName(kind),
      level: "active",
      thread: null,
      actions: [],
      url: href ? `https://oxy.test${href}` : null,
      ...over
    };
    items.push(item);
    items.sort((a, b) => b.created_at.localeCompare(a.created_at));
    return item;
  }

  /** The server's filters: `app_id` and `source` narrow by app, then category, kind, thread. */
  const inScope = (q: Record<string, string>) => (i: NotificationItem) => {
    if (options.legacy) return true;
    if (q.app_id && i.app?.id !== q.app_id) return false;
    if (q.source && (q.source === "oxygen" ? i.app !== null : i.app?.id !== q.source)) {
      return false;
    }
    if (q.kind && i.kind !== q.kind) return false;
    if (q.thread && i.thread !== q.thread) return false;
    const path = i.category?.path ?? GENERAL_CATEGORY;
    return !q.category || path === q.category || path.startsWith(`${q.category}/`);
  };
  const unreadIn = (q: Record<string, string>) =>
    items.filter(inScope(q)).filter((i) => i.read_at === null).length;
  const wire = (i: NotificationItem): NotificationItem => {
    if (!options.legacy) return i;
    const old = { ...i };
    delete old.link;
    for (const field of CATALOGUE_FIELDS) delete old[field];
    return old;
  };

  /** One page of the list, newest first: after the item `before` names, at most `limit`. */
  function list(q: Record<string, string>): Response {
    if (q.category && !q.source && !options.legacy) {
      return json({ message: "category needs source" }, 400);
    }
    const limit = Math.min(MAX_PAGE, Math.max(1, Number(q.limit) || 50));
    const unreadOnly = q.unread_only !== "false";
    let matching = items.filter(inScope(q)).filter((i) => !unreadOnly || i.read_at === null);
    if (q.before) matching = matching.slice(matching.findIndex((i) => i.id === q.before) + 1);
    const page = matching.slice(0, limit);
    const last = page[page.length - 1];
    // Unread within the filter, `before` aside — as the server counts it.
    const body: Record<string, unknown> = { unread: unreadIn(q), items: page.map(wire) };
    if (!options.legacy) body.next_before = matching.length > limit && last ? last.id : null;
    return json(body);
  }

  function answer(method: string, path: string, q: Record<string, string>): Response {
    if (options.status) return json({ message: `HTTP ${options.status}` }, options.status);
    if (method === "POST" && options.writeStatus) {
      return json({ message: "EnvironmentRefused" }, options.writeStatus);
    }
    if (method === "GET" && path === "/api/notifications/unread") {
      return json({ unread: unreadIn(q) });
    }
    if (method === "GET" && path === "/api/notifications") return list(q);
    if (method === "POST" && path === "/api/notifications/read-all") {
      const now = new Date().toISOString();
      for (const i of items.filter(inScope(q))) if (i.read_at === null) i.read_at = now;
      return new Response(null, { status: 204 });
    }
    const one = /^\/api\/notifications\/([^/]+)\/read$/.exec(path);
    if (method === "POST" && one) {
      const target = items.find((i) => i.id === decodeURIComponent(one[1]));
      if (!target) return json({ message: "not found" }, 404);
      if (target.read_at === null) target.read_at = new Date().toISOString();
      return new Response(null, { status: 204 });
    }
    return json({ message: "not found" }, 404);
  }

  const fetcher = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const { path, query } = parse(input);
    if (path !== "/api/notifications" && !path.startsWith("/api/notifications/")) {
      return options.fallback ? options.fallback(input, init) : json({ message: "not found" }, 404);
    }
    const method = (init?.method ?? "GET").toUpperCase();
    requests.push({ method, path, query });
    return answer(method, path, query);
  }) as typeof fetch;

  for (const item of options.items ?? []) push(item);

  return {
    fetcher,
    items,
    requests,
    push,
    receive: (input) => {
      const kind = String(input.kind);
      const thread = text(input.thread);
      return push({
        kind,
        title: String(input.title),
        body: text(input.body),
        link: text(input.link),
        thread: thread === null ? null : threadKey(thread),
        ...resolveNotificationKind(options.manifest, kind)
      });
    },
    unread: (id) => unreadIn(id ? { app_id: id } : {})
  };
}
