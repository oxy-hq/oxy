/** A channel a notification can reach someone on, besides the inbox itself. */
export type NotificationChannel = "push" | "email" | "slack";

/**
 * How loudly a kind interrupts. Only `critical` ignores mutes; mobile maps the
 * rest to iOS interruption levels and Android importance.
 */
export type NotificationLevel = "passive" | "active" | "time_sensitive" | "critical";

/** The custom app a notification came from. */
export interface NotificationApp {
  id: string;
  slug: string;
  name: string;
}

/**
 * A button a kind offers. No function: "open", which follows the notification's
 * link. With one (`runs_function`), the server calls the app's function as the
 * signed-in person.
 */
export interface NotificationAction {
  id: string;
  label: string;
  destructive: boolean;
  runs_function: boolean;
}

/** Where a notification sits in its source's tree. */
export interface NotificationCategoryRef {
  /** `operations/food-safety`; `general` for a kind with no category. */
  path: string;
  /** Root → leaf, e.g. `["Store operations", "Food safety"]`. */
  labels: string[];
}

/** One row of the inbox (`GET /notifications`). */
export interface InboxItem {
  id: string;
  org_id: string;
  kind: string;
  created_at: string;
  read_at: string | null;
  /** `null` when the org's store could not be read right now. */
  title: string | null;
  body: string | null;
  /**
   * Relative URL. An app's lands in the app (`/customer-apps/<org>/<app>/…`); a
   * platform notice's is a path inside this SPA.
   */
  href: string | null;
  /**
   * The path exactly as the app passed it to `ctx.notify` (e.g. `/bookings/1`),
   * for the app's own router. `null` for a platform notice. Oxygen follows `href`.
   */
  link: string | null;
  /** `null` for a platform notice, whose `kind` starts with `oxy.`. */
  app: NotificationApp | null;
  // The fields below are absent from a server older than notification
  // categories; every reader treats a missing one as "nothing to add".
  category?: NotificationCategoryRef;
  kind_label?: string;
  level?: NotificationLevel;
  /** Opaque: items sharing one collapse into the newest. */
  thread?: string | null;
  actions?: NotificationAction[];
  /** Absolute deep link, for mobile and sharing. Oxygen follows `href`. */
  url?: string | null;
}

export interface InboxResponse {
  unread: number;
  items: InboxItem[];
  /** The cursor for the next (older) page; absent or `null` when there is none. */
  next_before?: string | null;
}

/**
 * What the inbox list is narrowed to. `source` is `oxygen` or an app id;
 * `category` (a subtree) needs `source`.
 */
export interface InboxFilter {
  source?: string;
  category?: string;
  kind?: string;
  thread?: string;
}

/** The answer to running a notification's action. */
export interface NotificationActionResult {
  ok: boolean;
  /** A function action's JSON answer, or `null`. */
  result?: unknown;
  /** An open action's destination. */
  url?: string;
}

/** A kind, the leaf of the catalogue tree. */
export interface KindNode {
  kind: string;
  label: string;
  description: string | null;
  level: NotificationLevel;
  /** `level === "critical"`: delivered whatever is muted. */
  critical: boolean;
  /** The channels this kind can be delivered on. */
  channels: NotificationChannel[];
  /** Mutes set at the kind itself. */
  muted: NotificationChannel[];
  actions: NotificationAction[];
  unread: number;
}

/** A category: a path segment of a source's tree. */
export interface CategoryNode {
  path: string;
  label: string;
  description: string | null;
  /** Mutes set at this node (a parent's are not repeated here). */
  muted: NotificationChannel[];
  unread: number;
  categories: CategoryNode[];
  kinds: KindNode[];
}

/** Oxygen, or one custom app: the root of a tree. */
export interface SourceNode {
  source: "oxygen" | "app";
  app: (NotificationApp & { org_slug: string }) | null;
  label: string;
  /** Mutes set on the whole source. */
  muted: NotificationChannel[];
  unread: number;
  categories: CategoryNode[];
}

/** `GET /notifications/catalog`: every source that can tell this person something. */
export interface NotificationCatalog {
  sources: SourceNode[];
  reach: NotificationReach;
}

export interface UnreadResponse {
  unread: number;
}

export interface NotificationReach {
  push_devices: number;
  email: boolean;
}

/** Mute or unmute one channel of one kind. */
export interface SetKindPreference {
  /** `null` for a platform kind. */
  app_id: string | null;
  kind: string;
  channel: NotificationChannel;
  muted: boolean;
}

/** Mute or unmute one channel of a whole source (`scope: ""`) or a category subtree. */
export interface SetScopePreference {
  /** `null` for Oxygen. */
  app_id: string | null;
  scope: string;
  channel: NotificationChannel;
  muted: boolean;
}

/** `PUT /notifications/preferences` takes exactly one of `kind` / `scope`. */
export type SetNotificationPreference = SetKindPreference | SetScopePreference;

export interface VapidKeyResponse {
  configured: boolean;
  /** base64url, uncompressed P-256 point. */
  public_key: string | null;
}

export interface RegisterDeviceInput {
  platform: "web";
  /** For web, the `PushSubscription` endpoint URL. */
  token: string;
  device_name?: string;
}
