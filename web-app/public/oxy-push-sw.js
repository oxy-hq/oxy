// Oxygen inbox push worker — served as-is at /oxy-push-sw.js, scope "/".
//
// Registered by src/libs/webPush.ts when someone turns on push in Settings →
// Notifications. A custom app's own worker (/customer-apps/<org>/<app>/__oxy/sw.js)
// has a narrower scope and its own registration, so the two never compete.
//
// The server sends a BODILESS push: no payload, so nothing to decrypt, and this
// worker cannot read the app's auth token to fetch what the notification says.
// So every push shows the same generic line; the inbox, behind sign-in, has the rest.
// No fetch handler on purpose — this worker never sits between a page and the network.
//
// The message types are mirrored in src/libs/webPush.ts.

const OPEN_INBOX_MESSAGE = "oxy:open-notifications";
const INBOX_CHANGED_MESSAGE = "oxy:inbox-changed";
const INBOX_URL = "/?notifications=open";

self.addEventListener("install", () => {
  // Nothing to cache, so nothing for an old version to keep consistent.
  self.skipWaiting();
});

/** Product tabs on this origin — not custom-app tabs, which have no inbox. */
async function productWindows() {
  const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
  return windows.filter((client) => !new URL(client.url).pathname.startsWith("/customer-apps/"));
}

self.addEventListener("push", (event) => {
  event.waitUntil(
    (async () => {
      await self.registration.showNotification("Oxygen", {
        body: "You have a new notification",
        tag: "oxy-inbox",
        renotify: true,
        data: { url: INBOX_URL }
      });
      // An open tab's badge is now stale; tell it rather than wait for its poll.
      for (const client of await productWindows()) {
        client.postMessage({ type: INBOX_CHANGED_MESSAGE });
      }
    })()
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = event.notification.data?.url ?? INBOX_URL;
  event.waitUntil(
    (async () => {
      const [existing] = await productWindows();
      if (existing) {
        await existing.focus();
        existing.postMessage({ type: OPEN_INBOX_MESSAGE });
        return;
      }
      await self.clients.openWindow(url);
    })()
  );
});
