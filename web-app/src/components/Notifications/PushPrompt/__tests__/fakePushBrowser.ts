import { vi } from "vitest";

/** A subscription as a browser hands it over, keys included. */
export const FAKE_SUBSCRIPTION = {
  endpoint: "https://push.example/sub/1",
  keys: { p256dh: "BPub", auth: "QXV0aA" }
};

interface FakeBrowserOptions {
  permission?: NotificationPermission;
  /** Whether Oxygen's worker already holds a subscription here. */
  subscribed?: boolean;
  /** `requestPermission`'s answer. */
  answer?: NotificationPermission;
}

/**
 * Just enough of a push-capable browser for the real `@/libs/webPush` to run:
 * `Notification`, `PushManager`, and a service worker container whose one
 * registration holds (or does not hold) a subscription. jsdom has none of them.
 * `requestPermission` updates `Notification.permission`, as a browser does.
 */
export function fakePushBrowser({
  permission = "default",
  subscribed = false,
  answer = "granted"
}: FakeBrowserOptions = {}) {
  const subscription = {
    ...FAKE_SUBSCRIPTION,
    options: { applicationServerKey: null },
    toJSON: () => FAKE_SUBSCRIPTION,
    unsubscribe: vi.fn().mockResolvedValue(true)
  };
  const pushManager = {
    getSubscription: vi.fn().mockResolvedValue(subscribed ? subscription : null),
    subscribe: vi.fn().mockResolvedValue(subscription)
  };
  const registration = { pushManager };
  const notification = {
    permission,
    requestPermission: vi.fn(() => {
      notification.permission = answer;
      return Promise.resolve(answer);
    })
  };
  const serviceWorker = {
    register: vi.fn().mockResolvedValue(registration),
    ready: Promise.resolve(registration),
    getRegistration: vi.fn().mockResolvedValue(subscribed ? registration : undefined),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn()
  };
  vi.stubGlobal("Notification", notification);
  vi.stubGlobal("PushManager", function PushManager() {});
  Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: serviceWorker });
  return { notification, serviceWorker, pushManager };
}

/** Undo `fakePushBrowser`: a browser with no push at all, as jsdom is. */
export function noPushBrowser() {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(navigator, "serviceWorker");
}
