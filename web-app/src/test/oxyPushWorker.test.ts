/**
 * Execution tests for `public/oxy-push-sw.js`, the inbox's push worker.
 *
 * Read off disk and evaluated with a fake `ServiceWorkerGlobalScope`, as
 * `customAppClientScripts.test.ts` does for the custom-app worker: it is a
 * self-installing script, not a module, and its job — turning a decrypted
 * payload into a notification, and a click into a tab — is only visible by
 * running it.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";

const SCRIPT = readFileSync(join(__dirname, "../../public/oxy-push-sw.js"), "utf8");
const ORIGIN = "https://oxy.example.com";

interface FakeWindow {
  url: string;
  focus: ReturnType<typeof vi.fn>;
  postMessage: ReturnType<typeof vi.fn>;
}

const tab = (path: string): FakeWindow => ({
  url: `${ORIGIN}${path}`,
  focus: vi.fn().mockResolvedValue(undefined),
  postMessage: vi.fn()
});

type Handler = (event: Record<string, unknown>) => void;

function loadWorker(windows: FakeWindow[] = []) {
  const handlers: Record<string, Handler> = {};
  const scope = {
    addEventListener: (type: string, fn: Handler) => {
      handlers[type] = fn;
    },
    skipWaiting: vi.fn(),
    registration: { showNotification: vi.fn().mockResolvedValue(undefined) },
    clients: {
      matchAll: vi.fn().mockResolvedValue(windows),
      openWindow: vi.fn().mockResolvedValue(null)
    }
  };
  // oxlint-disable-next-line typescript/no-implied-eval -- the worker is a self-installing script, not a module: evaluating it is the test
  new Function("self", SCRIPT)(scope);

  async function fire(type: string, event: Record<string, unknown>) {
    let work: Promise<unknown> = Promise.resolve();
    handlers[type]({
      ...event,
      waitUntil: (p: Promise<unknown>) => {
        work = p;
      }
    });
    await work;
  }

  /** A push whose decrypted body is `payload`; `undefined` is a bodiless push. */
  async function push(payload?: unknown) {
    const data =
      payload === undefined
        ? null
        : {
            json: () => {
              if (typeof payload === "string") return JSON.parse(payload);
              return payload;
            }
          };
    await fire("push", { data });
    const [title, options] = scope.registration.showNotification.mock.calls.at(-1) ?? [];
    return { title, options };
  }

  async function click(data: unknown) {
    const close = vi.fn();
    await fire("notificationclick", { notification: { data, close } });
    expect(close).toHaveBeenCalled();
  }

  return { scope, push, click };
}

const PAYLOAD = {
  v: 1,
  id: "6f1c0c1e-0000-4000-8000-000000000001",
  app_name: "Store Ops",
  kind_label: "Temperature alarm",
  href: "/notifications?source=app-1&kind=temp_alarm"
};

describe("oxy-push-sw.js push", () => {
  it("names the app and the kind, and remembers where a click lands", async () => {
    const { push } = loadWorker();
    const { title, options } = await push(PAYLOAD);
    expect(title).toBe("Store Ops");
    expect(options).toMatchObject({
      body: "Temperature alarm",
      icon: "/icons/icon-192.png",
      tag: PAYLOAD.id,
      data: { url: PAYLOAD.href, href: true }
    });
  });

  it("tags a thread's pushes alike, so the newest replaces the rest", async () => {
    const { push } = loadWorker();
    const { options } = await push({ ...PAYLOAD, thread: "ab12" });
    expect(options).toMatchObject({ tag: "ab12", renotify: true });
  });

  it("says Oxygen when no app is named", async () => {
    const { push } = loadWorker();
    const { title } = await push({ ...PAYLOAD, app_name: "" });
    expect(title).toBe("Oxygen");
  });

  it("keeps the generic line for a bodiless push, an unknown version or a broken payload", async () => {
    const { push } = loadWorker();
    for (const payload of [undefined, { ...PAYLOAD, v: 2 }, "{not json", null]) {
      const { title, options } = await push(payload);
      expect(title, JSON.stringify(payload)).toBe("Oxygen");
      expect(options).toMatchObject({
        body: "You have a new notification",
        tag: "oxy-inbox",
        data: { url: "/?notifications=open" }
      });
      expect(options.data.href).toBeUndefined();
    }
  });

  it("never lands a click off this site, whatever the payload says", async () => {
    const { push } = loadWorker();
    for (const href of ["https://evil.example/", "//evil.example/x", "/\\evil.example", 42]) {
      const { options } = await push({ ...PAYLOAD, href });
      expect(options.data.url, String(href)).toBe("/notifications");
    }
  });

  it("tells open product tabs their badge is stale, and not custom-app tabs", async () => {
    const product = tab("/acme/workspaces/ws-1");
    const app = tab("/customer-apps/acme/store-ops/");
    const { push } = loadWorker([product, app]);
    await push(PAYLOAD);
    expect(product.postMessage).toHaveBeenCalledWith({ type: "oxy:inbox-changed" });
    expect(app.postMessage).not.toHaveBeenCalled();
  });
});

describe("oxy-push-sw.js notificationclick", () => {
  it("opens the push's href when no product tab is open", async () => {
    const { scope, push, click } = loadWorker([tab("/customer-apps/acme/store-ops/")]);
    const { options } = await push(PAYLOAD);
    await click(options.data);
    expect(scope.clients.openWindow).toHaveBeenCalledWith(PAYLOAD.href);
  });

  it("sends an open tab to the href", async () => {
    const product = tab("/acme/workspaces/ws-1");
    const { scope, push, click } = loadWorker([product]);
    const { options } = await push(PAYLOAD);
    await click(options.data);
    expect(product.focus).toHaveBeenCalled();
    expect(product.postMessage).toHaveBeenLastCalledWith({
      type: "oxy:open-notifications",
      href: PAYLOAD.href
    });
    expect(scope.clients.openWindow).not.toHaveBeenCalled();
  });

  it("opens the bell for a bodiless push, as before", async () => {
    const product = tab("/acme/workspaces/ws-1");
    const { push, click } = loadWorker([product]);
    const { options } = await push(undefined);
    await click(options.data);
    expect(product.postMessage).toHaveBeenLastCalledWith({ type: "oxy:open-notifications" });
  });

  it("opens the inbox for a notification with no data at all", async () => {
    const { scope, click } = loadWorker();
    await click(null);
    expect(scope.clients.openWindow).toHaveBeenCalledWith("/?notifications=open");
  });
});
