/** A channel a notification can reach someone on, besides the inbox itself. */
export type NotificationChannel = "push" | "email" | "slack";

/** The custom app a notification came from. */
export interface NotificationApp {
  id: string;
  slug: string;
  name: string;
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
}

export interface InboxResponse {
  unread: number;
  items: InboxItem[];
}

export interface UnreadResponse {
  unread: number;
}

export interface KindPreference {
  kind: string;
  muted: NotificationChannel[];
  /** A critical kind is delivered whatever is muted. */
  critical: boolean;
  /**
   * The channels this kind can reach someone on: the only switches that do
   * anything. Every channel for an app's kind; a fixed set for Oxygen's own.
   * Absent from a server older than the field, which means every channel.
   */
  channels?: NotificationChannel[];
}

export interface AppPreferences {
  app: NotificationApp;
  kinds: KindPreference[];
}

export interface NotificationReach {
  push_devices: number;
  email: boolean;
}

export interface NotificationPreferences {
  apps: AppPreferences[];
  /** Oxygen's own kinds (`oxy.*`) this person has received. */
  platform: KindPreference[];
  reach: NotificationReach;
}

export interface SetNotificationPreference {
  /** `null` for a platform kind. */
  app_id: string | null;
  kind: string;
  channel: NotificationChannel;
  muted: boolean;
}

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
