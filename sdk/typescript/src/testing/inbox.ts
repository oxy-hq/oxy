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
// function test sent (`state.notifications[i].input`) is what the bell shows.
// Pure TypeScript, no React, like the rest of this entry.

import type { NotifyInput } from "../custom-app/function-context";
import type { NotificationApp, NotificationItem } from "../custom-app/notifications";

export interface TestInboxOptions {
  /** The app `app_id` narrows to, and the default `app` of a new item. Default `"app-test"`. */
  appId?: string;
  /** The default app's slug, and the org slug in its `href`. */
  appSlug?: string;
  orgSlug?: string;
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
   * Behave as a server older than the `app_id` filter: ignore it, and send
   * items without `link`.
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
  | Pick<NotifyInput, "kind" | "title" | "body" | "link">
  | Readonly<Record<string, unknown>>;

const MAX_PAGE = 100;

const text = (v: unknown): string | null => (typeof v === "string" ? v : null);

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
    const item: NotificationItem = {
      id: `notification-${seq}`,
      org_id: "org-test",
      kind: "test",
      created_at: new Date(Date.UTC(2026, 0, 1, 0, 0, seq)).toISOString(),
      read_at: null,
      title: `Notification ${seq}`,
      body: null,
      href,
      link,
      app: itemApp,
      ...over
    };
    items.push(item);
    items.sort((a, b) => b.created_at.localeCompare(a.created_at));
    return item;
  }

  const inScope = (q: Record<string, string>) => (i: NotificationItem) =>
    options.legacy || !q.app_id || i.app?.id === q.app_id;
  const unreadIn = (q: Record<string, string>) =>
    items.filter(inScope(q)).filter((i) => i.read_at === null).length;
  const wire = (i: NotificationItem): NotificationItem => {
    if (!options.legacy) return i;
    const old = { ...i };
    delete old.link;
    return old;
  };

  function answer(method: string, path: string, q: Record<string, string>): Response {
    if (options.status) return json({ message: `HTTP ${options.status}` }, options.status);
    if (method === "POST" && options.writeStatus) {
      return json({ message: "EnvironmentRefused" }, options.writeStatus);
    }
    if (method === "GET" && path === "/api/notifications/unread") {
      return json({ unread: unreadIn(q) });
    }
    if (method === "GET" && path === "/api/notifications") {
      const limit = Math.min(MAX_PAGE, Math.max(1, Number(q.limit) || 50));
      const unreadOnly = q.unread_only !== "false";
      const page = items
        .filter(inScope(q))
        .filter((i) => !unreadOnly || i.read_at === null)
        .slice(0, limit);
      return json({ unread: unreadIn(q), items: page.map(wire) });
    }
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
    receive: (input) =>
      push({
        kind: String(input.kind),
        title: String(input.title),
        body: text(input.body),
        link: text(input.link)
      }),
    unread: (id) => unreadIn(id ? { app_id: id } : {})
  };
}
