// The in-app inbox, React-free: wire types, the four requests, and where a
// tapped notification goes. `useNotifications` (notifications-hooks.tsx) and
// `<NotificationsBell />` (notifications-bell.tsx) are thin over this.
//
// A custom app opens in its own tab, never inside Oxygen's shell, so a person
// who lives in one app — crew on a kiosk, a single-app user — never sees
// Oxygen's bell. The app's host forwards `/api/*` to the product API with the
// app's own session, so the app reads the same inbox the product does
// (`crates/app/src/server/api/notifications/`). Design:
// internal-docs/custom-app-notifications.md.

/** The custom app a notification came from. */
export interface NotificationApp {
  id: string;
  slug: string;
  name: string;
}

/** One row of the inbox (`GET /api/notifications`), as the server sends it. */
export interface NotificationItem {
  id: string;
  org_id: string;
  kind: string;
  created_at: string;
  read_at: string | null;
  /** `null` when the org's store could not be read right now. */
  title: string | null;
  body: string | null;
  /**
   * Where Oxygen opens it: an app's lands in the app
   * (`/customer-apps/<org>/<app>/…`), a platform notice's is a product path.
   */
  href: string | null;
  /**
   * The path inside the app exactly as the app passed it to `ctx.notify`
   * (`/bookings/1`), for the app's own router. `null` when it passed none, and
   * for a platform notice. Absent (`undefined`) on a server older than the
   * field — which also cannot narrow the inbox to one app.
   */
  link?: string | null;
  /** `null` for a platform notice. */
  app: NotificationApp | null;
}

/** `GET /api/notifications`. */
export interface NotificationsPage {
  unread: number;
  items: NotificationItem[];
}

/** `"app"`: this app's notifications. `"all"`: the person's whole inbox. */
export type NotificationsScope = "app" | "all";

/**
 * What an inbox request came to. `unavailable`: this server or session has no
 * inbox. `status`: the HTTP status of a refusal (absent for a dropped
 * connection).
 */
export type InboxOutcome<T> =
  | { ok: true; value: T }
  | { ok: false; unavailable: boolean; error: Error; status?: number };

/**
 * The status a staging or `dev-*` app host answers both writes with
 * (`EnvironmentRefused`): read state carries no environment, so a staging
 * "mark all read" would clear production's unread.
 */
export const WRITE_REFUSED = 403;

type Fetcher = typeof fetch;

/**
 * Statuses that mean "there is no inbox here", not "the inbox failed": a
 * server that predates the routes (404/405), or nobody signed in who can read
 * one (401/403). A bell renders nothing for these instead of an error.
 */
const UNAVAILABLE = new Set([401, 403, 404, 405]);

async function errorOf(resp: Response): Promise<Error> {
  let detail = "";
  try {
    detail = (await resp.text()).slice(0, 240);
  } catch {
    // The status is enough.
  }
  return new Error(`notifications: HTTP ${resp.status}${detail ? ` — ${detail}` : ""}`);
}

async function request<T>(
  fetcher: Fetcher,
  url: string,
  init: RequestInit,
  parse: (resp: Response) => Promise<T>
): Promise<InboxOutcome<T>> {
  let resp: Response;
  try {
    resp = await fetcher(url, init);
  } catch (e) {
    return { ok: false, unavailable: false, error: e instanceof Error ? e : new Error(String(e)) };
  }
  if (!resp.ok) {
    return {
      ok: false,
      unavailable: UNAVAILABLE.has(resp.status),
      error: await errorOf(resp),
      status: resp.status
    };
  }
  try {
    return { ok: true, value: await parse(resp) };
  } catch {
    // A 200 that is not the inbox's JSON is a host answering every path with
    // a page (an app host does): there is no inbox behind it.
    return {
      ok: false,
      unavailable: true,
      error: new Error(`notifications: ${url} did not answer with the inbox`)
    };
  }
}

async function json<T>(resp: Response): Promise<T> {
  return (await resp.json()) as T;
}

function unreadOf(body: unknown): number {
  const n = (body as { unread?: unknown } | null)?.unread;
  if (typeof n !== "number" || !Number.isFinite(n)) throw new Error("no unread count");
  return n;
}

function pageOf(body: unknown): NotificationsPage {
  const items = (body as { items?: unknown } | null)?.items;
  if (!Array.isArray(items)) throw new Error("no items");
  return { unread: unreadOf(body), items: items as NotificationItem[] };
}

/** The four inbox requests, narrowed to one app when `appId` is set. */
export interface InboxClient {
  /** The badge: Oxy's own database only, so polling never wakes an org's store. */
  unread(): Promise<InboxOutcome<number>>;
  /** Newest first, read and unread. */
  list(limit: number): Promise<InboxOutcome<NotificationsPage>>;
  markRead(id: string): Promise<InboxOutcome<void>>;
  markAllRead(): Promise<InboxOutcome<void>>;
}

/**
 * `apiBaseUrl` is the resolved manifest's (empty when oxy serves the bundle);
 * `fetcher` is the provider's, so `backendUrl` and credentials apply.
 */
export function createInboxClient(
  fetcher: Fetcher,
  apiBaseUrl: string,
  appId: string | undefined
): InboxClient {
  const root = `${apiBaseUrl.replace(/\/+$/, "")}/api/notifications`;
  const scoped = (path: string, extra: Record<string, string> = {}): string => {
    const params = new URLSearchParams(extra);
    if (appId) params.set("app_id", appId);
    const query = params.toString();
    return `${root}${path}${query ? `?${query}` : ""}`;
  };
  const get = { method: "GET" };
  const post = { method: "POST" };
  return {
    unread: () => request(fetcher, scoped("/unread"), get, async (r) => unreadOf(await json(r))),
    list: (limit) =>
      request(fetcher, scoped("", { unread_only: "false", limit: String(limit) }), get, async (r) =>
        pageOf(await json(r))
      ),
    markRead: (id) =>
      request(fetcher, `${root}/${encodeURIComponent(id)}/read`, post, async () => undefined),
    markAllRead: () => request(fetcher, scoped("/read-all"), post, async () => undefined)
  };
}

/**
 * Did this page come from a server that narrows the inbox to one app? The
 * `link` field shipped with the `app_id` filter, so an item without it is from
 * a server that ignored the filter — whose "mark all read" would clear every
 * app's notifications. `null` when there is nothing to judge by.
 */
export function serverScopesToApp(items: NotificationItem[]): boolean | null {
  if (items.length === 0) return null;
  return items.every((item) => item.link !== undefined);
}

// ── Where a tapped notification goes ────────────────────────────────────────

/** A path inside an app, and nothing that could leave it. */
function safeAppPath(path: string | null | undefined): string | null {
  if (!path?.startsWith("/") || path.startsWith("//") || path.includes("\\")) {
    return null;
  }
  return path;
}

const APP_HREF = /^\/customer-apps\/([^/?#]+)\/([^/?#]+)(?=[/?#]|$)(.*)$/;

/**
 * The app's base path in the address bar this page was loaded under: the main
 * host's `/customer-apps/<org>/<app>/`, an org subdomain's `/a/<app>/`, or `/`
 * on the app's own subdomain (and `pnpm dev` at the root).
 */
export function appBasePath(pathname: string, orgSlug: string, appSlug: string): string {
  const prefixes = [
    orgSlug && appSlug ? `/customer-apps/${orgSlug}/${appSlug}` : "",
    appSlug ? `/a/${appSlug}` : ""
  ];
  for (const prefix of prefixes) {
    if (prefix && (pathname === prefix || pathname.startsWith(`${prefix}/`))) return `${prefix}/`;
  }
  return "/";
}

/** `link` under `base`: `("/a/x/", "/bookings/1")` → `/a/x/bookings/1`. */
export function joinAppPath(base: string, link: string): string {
  return `${base.replace(/\/+$/, "")}${link}`;
}

/** The window an app's link opens in — one tab per app, as Oxygen does it. */
export function appWindowName(orgSlug: string, appSlug: string): string {
  return `oxy-app-${orgSlug}-${appSlug}`;
}

/**
 * Leaving the page. A seam, not an option: jsdom cannot navigate, and its
 * `location` cannot be spied on, so the bell's tests replace these.
 */
export const pageNavigation = {
  assign: (url: string): void => window.location.assign(url),
  open: (url: string, target: string, features?: string): void => {
    window.open(url, target, features);
  }
};

/** Where tapping an item takes the person. */
export type NotificationTarget =
  /** This app: `link` is a path inside it, for its router. */
  | { kind: "app"; link: string }
  /** Somewhere else: open `url` in the window named `window` (a new tab when null). */
  | { kind: "away"; url: string; window: string | null }
  /** Nothing to open; tapping only marks it read. */
  | { kind: "none" };

export interface TargetContext {
  /** This app's id (`window.__OXY_APP__.appId`); undefined in `pnpm dev`. */
  appId: string | undefined;
  /**
   * The Oxygen origin for a relative `href`, when this page is not served by
   * it (a standalone dev server). Unset: the `href` as the server sent it.
   */
  productBaseUrl?: string;
}

/** The path inside the app, from a server older than `link`: `href` minus the app's prefix. */
function linkFromHref(href: string | null): string | null {
  const rest = href ? APP_HREF.exec(href)?.[3] : undefined;
  if (!rest || rest === "/") return null;
  return safeAppPath(rest.startsWith("/") ? rest : `/${rest}`);
}

function awayTarget(href: string | null, productBaseUrl?: string): NotificationTarget {
  if (!href) return { kind: "none" };
  if (/^https?:\/\//i.test(href)) return { kind: "away", url: href, window: null };
  if (!safeAppPath(href)) return { kind: "none" };
  const app = APP_HREF.exec(href);
  const url = productBaseUrl ? `${productBaseUrl.replace(/\/+$/, "")}${href}` : href;
  return { kind: "away", url, window: app ? appWindowName(app[1], app[2]) : null };
}

/**
 * This app's notification opens inside it, by its own `link`; anything else —
 * another app's, a platform notice — opens where Oxygen would open it.
 */
export function notificationTarget(item: NotificationItem, ctx: TargetContext): NotificationTarget {
  const own = ctx.appId !== undefined && item.app?.id === ctx.appId;
  if (!own) return awayTarget(item.href, ctx.productBaseUrl);
  const link = item.link === undefined ? linkFromHref(item.href) : safeAppPath(item.link);
  return link ? { kind: "app", link } : { kind: "none" };
}

// ── Display ─────────────────────────────────────────────────────────────────

/** The badge text: nothing at zero, capped at "99+". */
export function badgeLabel(count: number): string | null {
  if (count <= 0) return null;
  return count > 99 ? "99+" : String(count);
}

/** "now", "5m", "3h", "2d", then a date. Empty for an unreadable timestamp. */
export function formatAge(iso: string, now: number = Date.now()): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return "";
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  if (seconds < 7 * 86400) return `${Math.floor(seconds / 86400)}d`;
  return new Date(at).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
