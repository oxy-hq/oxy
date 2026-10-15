/**
 * Web Push for the inbox, browser side.
 *
 * The server sends a **bodiless** push — no payload, so no encryption keys — to a
 * subscription's endpoint, and `public/oxy-push-sw.js` turns it into a generic
 * "You have a new notification". What it says stays in the inbox, behind sign-in:
 * the worker cannot read the app's token, so it could not fetch it anyway.
 *
 * The worker sits at the origin root with scope `/`. A custom app's own worker
 * (`/customer-apps/<org>/<app>/__oxy/sw.js`) has a narrower scope and its own
 * registration, so the two never compete: the longest matching scope controls a
 * page, and a push subscription belongs to one registration.
 */

/** Served as-is from `web-app/public`. */
export const PUSH_WORKER_URL = "/oxy-push-sw.js";

/** Worker → page: open the inbox (a notification was clicked). Mirrored in the worker. */
export const OPEN_INBOX_MESSAGE = "oxy:open-notifications";

/** Worker → page: a push arrived, so the badge is stale. Mirrored in the worker. */
export const INBOX_CHANGED_MESSAGE = "oxy:inbox-changed";

/** Whether this browser can take a push at all. */
export function pushSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/** What `pushNeedsHomeScreen` reads from the browser. */
export interface PushEnvironment {
  userAgent: string;
  platform: string;
  maxTouchPoints: number;
  /** Running as an installed web app (`display-mode: standalone`, or iOS's `navigator.standalone`). */
  standalone: boolean;
}

/** `iPhone OS 17_5 like Mac OS X`, `CPU OS 16_3 like Mac OS X` — not a Mac's `OS X 10_15_7`. */
const IOS_VERSION = /\bOS (\d+)_(\d+)(?:_\d+)? like Mac OS X/;

/**
 * True for an iPhone or iPad browsing in a tab. iOS gives push only to a web app
 * added to the Home Screen (iOS 16.4 and later), so there the way to turn it on
 * is to install Oxygen first. False inside the installed app, on any other
 * device, and on an iOS too old to have web push at all.
 */
export function pushNeedsHomeScreen(env: PushEnvironment): boolean {
  if (env.standalone) return false;
  // iPadOS asks for the desktop site by default, so it reads as a Mac with a touchscreen.
  const iPadAsMac = env.platform === "MacIntel" && env.maxTouchPoints > 1;
  if (!/iPhone|iPad|iPod/.test(env.userAgent) && !iPadAsMac) return false;
  const version = IOS_VERSION.exec(env.userAgent);
  if (!version) return true;
  const [major, minor] = [Number(version[1]), Number(version[2])];
  return major > 16 || (major === 16 && minor >= 4);
}

/** `pushNeedsHomeScreen` for the browser this page is running in. */
export function pushNeedsHomeScreenHere(): boolean {
  if (typeof window === "undefined") return false;
  const nav = navigator as Navigator & { standalone?: boolean };
  return pushNeedsHomeScreen({
    userAgent: nav.userAgent,
    platform: nav.platform,
    maxTouchPoints: nav.maxTouchPoints,
    standalone: nav.standalone === true || window.matchMedia("(display-mode: standalone)").matches
  });
}

/** A VAPID public key (base64url, unpadded) as the bytes `subscribe` wants. */
export function urlBase64ToUint8Array(base64Url: string): Uint8Array<ArrayBuffer> {
  const padding = "=".repeat((4 - (base64Url.length % 4)) % 4);
  const base64 = (base64Url + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

function sameKey(current: ArrayBuffer | null, wanted: Uint8Array): boolean {
  if (!current || current.byteLength !== wanted.byteLength) return false;
  const have = new Uint8Array(current);
  return have.every((byte, i) => byte === wanted[i]);
}

/**
 * Register the worker, ask for permission, and subscribe this browser. Returns the
 * subscription's endpoint, which is the whole device token for a bodiless push.
 *
 * A subscription made under a different key (the deployment rotated its VAPID
 * pair) is dropped first: `subscribe` refuses to change keys in place, and a push
 * signed with the new key would be rejected for the old subscription.
 */
export async function subscribeThisDevice(publicKey: string): Promise<string> {
  const permission = await Notification.requestPermission();
  if (permission !== "granted") {
    throw new Error("Notifications are blocked for this site in your browser settings.");
  }
  await navigator.serviceWorker.register(PUSH_WORKER_URL, { scope: "/" });
  // `ready` resolves once the worker is active — `subscribe` throws on one that is
  // still installing. Scope `/` covers every product page, so it is ours.
  const registration = await navigator.serviceWorker.ready;
  const key = urlBase64ToUint8Array(publicKey);
  const existing = await registration.pushManager.getSubscription();
  if (existing && sameKey(existing.options.applicationServerKey, key)) return existing.endpoint;
  if (existing) await existing.unsubscribe();
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: key
  });
  return subscription.endpoint;
}
