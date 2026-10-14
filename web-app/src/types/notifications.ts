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
  /** `null` for a platform notice. */
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
  reach: NotificationReach;
}

export interface SetNotificationPreference {
  app_id: string;
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
