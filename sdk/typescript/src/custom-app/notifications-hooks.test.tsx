// @vitest-environment jsdom
//
// `useNotifications` against the fake inbox from `@oxy-hq/sdk/testing`,
// mounted for real behind `<OxyAppProvider>`: what it asks the server, when it
// stops asking, and that it never throws where there is no inbox.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestInbox, type TestInbox } from "../testing/inbox";
import { APP_ID, mount, type Page, servePage, setHidden, settle } from "./notifications.harness";
import {
  type UseNotificationsOptions,
  type UseNotificationsResult,
  useNotifications
} from "./notifications-hooks";

let page: Page | undefined;
let last: UseNotificationsResult;

function Probe(props: { opts?: UseNotificationsOptions }): null {
  last = useNotifications(props.opts);
  return null;
}

async function render(inbox: TestInbox, opts?: UseNotificationsOptions): Promise<void> {
  page = await mount(<Probe opts={opts} />, inbox.fetcher);
}

const lastRequest = (inbox: TestInbox) => inbox.requests[inbox.requests.length - 1];

const unreadPolls = (inbox: TestInbox) =>
  inbox.requests.filter((r) => r.path === "/api/notifications/unread");

beforeEach(() => {
  vi.useFakeTimers();
  servePage();
});

afterEach(async () => {
  await page?.unmount();
  page = undefined;
  vi.useRealTimers();
});

describe("useNotifications — scope", () => {
  it("narrows to this app by default", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push();
    inbox.push({ app: { id: "app-2", slug: "lists", name: "Lists" } });
    await render(inbox);
    expect(unreadPolls(inbox)[0]?.query).toEqual({ app_id: APP_ID });
    expect(last.unread).toBe(1);
    expect(last.available).toBe(true);
  });

  it('drops the filter for scope "all"', async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push();
    inbox.push({ app: null, link: null, kind: "oxy.billing_past_due" });
    await render(inbox, { scope: "all" });
    expect(unreadPolls(inbox)[0]?.query).toEqual({});
    expect(last.unread).toBe(2);
  });

  it('asks nothing under scope "app" on a page with no app id, and says why', async () => {
    servePage(null);
    const inbox = createTestInbox({ appId: APP_ID });
    await render(inbox);
    await settle(60_000);
    expect(inbox.requests).toEqual([]);
    expect(last.available).toBe(false);
    expect(last.error?.message).toContain('scope: "all"');
  });
});

describe("useNotifications — polling", () => {
  it("polls the unread count every 30s, and only that", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    await render(inbox);
    expect(unreadPolls(inbox)).toHaveLength(1);
    inbox.push();
    await settle(30_000);
    expect(unreadPolls(inbox)).toHaveLength(2);
    expect(last.unread).toBe(1);
    expect(inbox.requests.every((r) => r.path === "/api/notifications/unread")).toBe(true);
  });

  it("pauses while the tab is hidden and catches up on coming back", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    await render(inbox);
    setHidden(true);
    await settle(120_000);
    expect(unreadPolls(inbox)).toHaveLength(1);
    inbox.push();
    setHidden(false);
    await settle();
    expect(unreadPolls(inbox)).toHaveLength(2);
    expect(last.unread).toBe(1);
  });
});

describe("useNotifications — reads and writes", () => {
  it("fetches the list only when asked, read items included", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push({ title: "older", read_at: "2026-01-01T00:00:00Z" });
    inbox.push({ title: "newer" });
    await render(inbox);
    expect(last.items).toEqual([]);
    await last.refresh();
    await settle();
    expect(last.items.map((i) => i.title)).toEqual(["newer", "older"]);
    const list = inbox.requests.find((r) => r.path === "/api/notifications");
    expect(list?.query).toMatchObject({ app_id: APP_ID, unread_only: "false" });
  });

  it("marks one read: one POST, the dot and the count follow", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    const a = inbox.push();
    inbox.push();
    await render(inbox);
    await last.refresh();
    await settle();
    await last.markRead(a.id);
    await settle();
    expect(lastRequest(inbox)).toMatchObject({
      method: "POST",
      path: `/api/notifications/${a.id}/read`
    });
    expect(last.unread).toBe(1);
    expect(last.items.find((i) => i.id === a.id)?.read_at).not.toBeNull();
  });

  it("marks all read for this app only", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push();
    const other = inbox.push({ app: { id: "app-2", slug: "lists", name: "Lists" } });
    await render(inbox);
    await last.refresh();
    await settle();
    await last.markAllRead();
    await settle();
    expect(lastRequest(inbox)).toMatchObject({
      method: "POST",
      path: "/api/notifications/read-all",
      query: { app_id: APP_ID }
    });
    expect(last.unread).toBe(0);
    expect(inbox.items.find((i) => i.id === other.id)?.read_at).toBeNull();
  });

  it("refreshes the open list when the polled count moves", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    await render(inbox);
    await last.refresh();
    await settle();
    expect(last.items).toHaveLength(0);
    inbox.push({ title: "arrived" });
    await settle(30_000);
    expect(last.items.map((i) => i.title)).toEqual(["arrived"]);
  });
});

describe("useNotifications — where there is no inbox", () => {
  for (const [status, what] of [
    [404, "a server without the routes"],
    [401, "a page nobody is signed in to"]
  ] as const) {
    it(`renders unavailable, never throws, and stops polling on ${what}`, async () => {
      const inbox = createTestInbox({ appId: APP_ID, status });
      await render(inbox);
      expect(last.available).toBe(false);
      expect(last.error?.message).toContain(`HTTP ${status}`);
      await expect(last.refresh()).resolves.toBeUndefined();
      await expect(last.markAllRead()).resolves.toBeUndefined();
      const before = unreadPolls(inbox).length;
      await settle(120_000);
      expect(unreadPolls(inbox)).toHaveLength(before);
    });
  }

  it("keeps the inbox through a failed poll and recovers on the next", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    let down = false;
    const flaky = ((input: RequestInfo | URL, init?: RequestInit) =>
      down
        ? Promise.resolve(new Response("boom", { status: 502 }))
        : inbox.fetcher(input, init)) as typeof fetch;
    page = await mount(<Probe />, flaky);
    expect(last.available).toBe(true);
    down = true;
    await settle(30_000);
    expect(last.available).toBe(true);
    expect(last.error?.message).toContain("HTTP 502");
    down = false;
    await settle(30_000);
    expect(last.error).toBeNull();
  });

  it("hides an app's inbox on a server that ignores app_id, and never clears every app's", async () => {
    const inbox = createTestInbox({ appId: APP_ID, legacy: true });
    inbox.push();
    const other = inbox.push({ app: { id: "app-2", slug: "lists", name: "Lists" } });
    await render(inbox);
    await last.markAllRead();
    await settle();
    expect(inbox.requests.some((r) => r.path === "/api/notifications/read-all")).toBe(false);
    expect(other.read_at).toBeNull();
    expect(last.available).toBe(false);
    expect(last.error?.message).toContain("cannot narrow");
  });
});

describe("useNotifications — staging and dev hosts", () => {
  const writes = (inbox: TestInbox) => inbox.requests.filter((r) => r.method === "POST");

  for (const environment of ["staging", "dev-ada"] as const) {
    it(`reads but never writes read state on a ${environment} host`, async () => {
      servePage(APP_ID, environment);
      const inbox = createTestInbox({ appId: APP_ID });
      const a = inbox.push();
      inbox.push();
      await render(inbox);
      expect(last.readOnly).toBe(true);
      await last.refresh();
      await settle();
      expect(last.items).toHaveLength(2);
      await last.markRead(a.id);
      await last.markAllRead();
      await settle();
      expect(writes(inbox)).toEqual([]);
      expect(last.unread).toBe(2);
      expect(last.items.every((i) => i.read_at === null)).toBe(true);
    });
  }

  it("writes on production, and on a server older than environments", async () => {
    for (const environment of ["production", undefined] as const) {
      servePage(APP_ID, environment);
      const inbox = createTestInbox({ appId: APP_ID });
      const a = inbox.push();
      await render(inbox);
      expect(last.readOnly).toBe(false);
      await last.markRead(a.id);
      await settle();
      expect(writes(inbox)).toHaveLength(1);
      await page?.unmount();
      page = undefined;
    }
  });

  it("takes a 403 on either write quietly: no error, the bell stays, the state re-syncs", async () => {
    const inbox = createTestInbox({ appId: APP_ID, writeStatus: 403 });
    const a = inbox.push();
    inbox.push();
    await render(inbox);
    await last.refresh();
    await settle();
    await last.markRead(a.id);
    await settle();
    await last.markAllRead();
    await settle();
    expect(writes(inbox).map((r) => r.path)).toEqual([
      `/api/notifications/${a.id}/read`,
      "/api/notifications/read-all"
    ]);
    expect(last.error).toBeNull();
    expect(last.available).toBe(true);
    expect(last.unread).toBe(2);
    expect(last.items.every((i) => i.read_at === null)).toBe(true);
  });

  it("still reports any other failed write", async () => {
    const inbox = createTestInbox({ appId: APP_ID, writeStatus: 500 });
    const a = inbox.push();
    await render(inbox);
    await last.markRead(a.id);
    await settle();
    expect(last.error?.message).toContain("HTTP 500");
    expect(last.available).toBe(true);
  });
});

describe("useNotifications — category, kind and thread", () => {
  const MANIFEST = {
    functions: { f: { notify: { kinds: { temp_alarm: {}, list_overdue: {} } } } },
    notifications: {
      categories: { "operations/food-safety": { label: "Food safety" } },
      kinds: { temp_alarm: { category: "operations/food-safety" } }
    }
  };

  function stocked(): { inbox: TestInbox; alarm: { id: string }; list: { id: string } } {
    const inbox = createTestInbox({ appId: APP_ID, manifest: MANIFEST });
    const alarm = inbox.receive({ kind: "temp_alarm", title: "alarm", thread: "store:1" });
    const list = inbox.receive({ kind: "list_overdue", title: "list" });
    return { inbox, alarm, list };
  }

  /**
   * A server with `app_id` but older than categories: it ignores the new
   * filters and sends no catalogue field.
   */
  function olderServer(inbox: TestInbox): typeof fetch {
    return (async (input: RequestInfo | URL, init?: RequestInit) => {
      const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const url = new URL(raw, "http://inbox.test");
      for (const key of ["source", "category", "kind", "thread"]) url.searchParams.delete(key);
      const resp = await inbox.fetcher(`${url.pathname}${url.search}`, init);
      if (url.pathname !== "/api/notifications") return resp;
      const body = (await resp.json()) as { unread: number; items: Record<string, unknown>[] };
      const items = body.items.map(
        ({ category, kind_label, level, thread, actions, url: u, ...rest }) => rest
      );
      return new Response(JSON.stringify({ unread: body.unread, items }), { status: 200 });
    }) as typeof fetch;
  }

  const listRequests = (inbox: TestInbox) =>
    inbox.requests.filter((r) => r.method === "GET" && r.path === "/api/notifications");

  it("asks for a category of this app (naming it as the source), a kind, a thread", async () => {
    const { inbox } = stocked();
    await render(inbox, { category: "operations", kind: "temp_alarm" });
    await last.refresh();
    await settle();
    expect(listRequests(inbox)[0]?.query).toMatchObject({
      app_id: APP_ID,
      source: APP_ID,
      category: "operations",
      kind: "temp_alarm"
    });
    expect(last.items.map((i) => i.title)).toEqual(["alarm"]);
    // The count narrows with the list, polled with the same filter.
    expect(last.unread).toBe(1);
    expect(unreadPolls(inbox)[0]?.query).toMatchObject({
      category: "operations",
      kind: "temp_alarm"
    });
  });

  it("filters by an item's thread", async () => {
    const { inbox, alarm } = stocked();
    const thread = inbox.items.find((i) => i.id === alarm.id)?.thread ?? "";
    await render(inbox, { thread });
    await last.refresh();
    await settle();
    expect(listRequests(inbox)[0]?.query.thread).toBe(thread);
    expect(last.items.map((i) => i.title)).toEqual(["alarm"]);
  });

  it("needs the app id for a category, even under scope all, and asks nothing without it", async () => {
    servePage(null);
    const { inbox } = stocked();
    await render(inbox, { scope: "all", category: "operations" });
    await settle(60_000);
    expect(inbox.requests).toEqual([]);
    expect(last.available).toBe(false);
    expect(last.error?.message).toContain("category");
  });

  it("filters by kind under scope all with no app id", async () => {
    servePage(null);
    const { inbox } = stocked();
    await render(inbox, { scope: "all", kind: "list_overdue" });
    await last.refresh();
    await settle();
    expect(listRequests(inbox)[0]?.query).toEqual({
      unread_only: "false",
      limit: "20",
      kind: "list_overdue"
    });
    expect(last.items.map((i) => i.title)).toEqual(["list"]);
  });

  it("marks all read with one filtered read-all, on a server that filters", async () => {
    const { inbox, list } = stocked();
    await render(inbox, { kind: "temp_alarm" });
    await last.refresh();
    await settle();
    await last.markAllRead();
    await settle();
    const writes = inbox.requests.filter((r) => r.method === "POST");
    expect(writes).toEqual([
      {
        method: "POST",
        path: "/api/notifications/read-all",
        query: { app_id: APP_ID, kind: "temp_alarm" }
      }
    ]);
    expect(inbox.items.find((i) => i.id === list.id)?.read_at).toBeNull();
    expect(last.unread).toBe(0);
    expect(inbox.unread(APP_ID)).toBe(1);
  });

  it("keeps an older server's other items out of the list", async () => {
    const { inbox } = stocked();
    page = await mount(<Probe opts={{ kind: "temp_alarm" }} />, olderServer(inbox));
    await last.refresh();
    await settle();
    expect(last.items.map((i) => i.title)).toEqual(["alarm"]);
  });

  it("marks each listed item on an older server, never a read-all that would clear the app", async () => {
    const { inbox, alarm, list } = stocked();
    page = await mount(<Probe opts={{ kind: "temp_alarm" }} />, olderServer(inbox));
    await last.markAllRead();
    await settle();
    const writes = inbox.requests.filter((r) => r.method === "POST").map((r) => r.path);
    expect(writes).toEqual([`/api/notifications/${alarm.id}/read`]);
    expect(inbox.items.find((i) => i.id === list.id)?.read_at).toBeNull();
    expect(inbox.items.find((i) => i.id === alarm.id)?.read_at).not.toBeNull();
  });
});
