// `createTestInbox` answers the inbox routes as the server does — the app
// filter, the unread-only default, read state — and takes a function test's
// `ctx.notify` payload as the item its recipient sees.

import { describe, expect, it } from "vitest";
import { createTestContext } from "./context";
import { createTestInbox, threadKey } from "./inbox";

async function get(inbox: ReturnType<typeof createTestInbox>, url: string): Promise<unknown> {
  return (await inbox.fetcher(url)).json();
}

describe("createTestInbox", () => {
  it("narrows by app_id and defaults to unread only, as the server does", async () => {
    const inbox = createTestInbox({ appId: "app-1" });
    inbox.push({ title: "mine" });
    inbox.push({ title: "read", read_at: "2026-01-01T00:00:00Z" });
    inbox.push({ title: "theirs", app: { id: "app-2", slug: "x", name: "X" } });
    expect(await get(inbox, "/api/notifications/unread?app_id=app-1")).toEqual({ unread: 1 });
    expect(await get(inbox, "/api/notifications/unread")).toEqual({ unread: 2 });
    const page = (await get(inbox, "/api/notifications?app_id=app-1")) as {
      items: { title: string }[];
    };
    expect(page.items.map((i) => i.title)).toEqual(["mine"]);
    const all = (await get(inbox, "/api/notifications?app_id=app-1&unread_only=false")) as {
      items: { title: string }[];
    };
    expect(all.items.map((i) => i.title)).toEqual(["read", "mine"]);
  });

  it("writes read state, and 404s someone else's id", async () => {
    const inbox = createTestInbox({ appId: "app-1" });
    const a = inbox.push();
    inbox.push({ app: { id: "app-2", slug: "x", name: "X" } });
    expect(
      (await inbox.fetcher(`/api/notifications/${a.id}/read`, { method: "POST" })).status
    ).toBe(204);
    expect((await inbox.fetcher("/api/notifications/nope/read", { method: "POST" })).status).toBe(
      404
    );
    await inbox.fetcher("/api/notifications/read-all?app_id=app-1", { method: "POST" });
    expect(inbox.unread("app-1")).toBe(0);
    expect(inbox.unread()).toBe(1);
  });

  it("acts as an older server: ignores app_id and sends no link or catalogue field", async () => {
    const inbox = createTestInbox({ appId: "app-1", legacy: true });
    inbox.push({ link: "/a" });
    inbox.push({ app: { id: "app-2", slug: "x", name: "X" } });
    const page = (await get(inbox, "/api/notifications?app_id=app-1&kind=nope")) as {
      items: Record<string, unknown>[];
      next_before?: unknown;
    };
    expect(page.items).toHaveLength(2);
    for (const field of ["link", "category", "kind_label", "level", "thread", "actions", "url"]) {
      expect(
        page.items.every((i) => !(field in i)),
        field
      ).toBe(true);
    }
    expect("next_before" in page).toBe(false);
  });

  it("shows what a function sent with ctx.notify", async () => {
    const t = createTestContext(
      { slug: "bookings", functions: { f: { notify: { kinds: { booking_created: {} } } } } },
      { function: "f" }
    );
    t.state.org.people.push({ id: "u-1", name: "Mo", role: "member", kind: "member" });
    await t.ctx.notify({
      to: { userId: "u-1" },
      kind: "booking_created",
      title: "New booking",
      body: "Table for 4",
      link: "/bookings/1"
    });
    const inbox = createTestInbox({ appId: "app-1", appSlug: "bookings", orgSlug: "acme" });
    const item = inbox.receive(t.state.notifications[0].input);
    expect(item).toMatchObject({
      kind: "booking_created",
      title: "New booking",
      body: "Table for 4",
      link: "/bookings/1",
      href: "/customer-apps/acme/bookings/bookings/1",
      app: { id: "app-1", slug: "bookings" },
      read_at: null
    });
  });

  it("answers every inbox route with `status`, and leaves other paths to `fallback`", async () => {
    const inbox = createTestInbox({
      status: 401,
      fallback: async () => new Response("manifest", { status: 200 })
    });
    expect((await inbox.fetcher("/api/notifications/unread")).status).toBe(401);
    expect(await (await inbox.fetcher("/oxy-app.json")).text()).toBe("manifest");
    expect(inbox.requests).toHaveLength(1);
  });
});

const MANIFEST = {
  slug: "store-ops",
  functions: {
    temps: { notify: { kinds: { temp_alarm: {}, list_overdue: {} } } },
    "ack-alarm": { route: true }
  },
  notifications: {
    categories: {
      operations: { label: "Store operations" },
      "operations/food-safety": { label: "Food safety" }
    },
    kinds: {
      temp_alarm: {
        category: "operations/food-safety",
        label: "Temperature alarm",
        level: "critical",
        actions: [{ id: "ack", label: "Acknowledge", function: "ack-alarm" }]
      },
      list_overdue: { category: "operations", level: "time_sensitive" }
    }
  }
};

type Page = { items: { id: string; title: string }[]; next_before: string | null };

describe("createTestInbox — the catalogue's fields", () => {
  it("completes a pushed item with the defaults a kind outside any catalogue gets", () => {
    const inbox = createTestInbox({ appId: "app-1", appSlug: "bookings", orgSlug: "acme" });
    expect(inbox.push({ kind: "booking_created" })).toMatchObject({
      category: { path: "general", labels: ["General"] },
      kind_label: "Booking created",
      level: "active",
      thread: null,
      actions: [],
      url: "https://oxy.test/customer-apps/acme/bookings/"
    });
    expect(inbox.push({ app: null }).url).toBeNull();
  });

  it("receives a ctx.notify payload as the catalogue places it, its thread hashed", () => {
    const inbox = createTestInbox({ appId: "app-1", manifest: MANIFEST });
    const item = inbox.receive({
      kind: "temp_alarm",
      title: "Walk-in at 9°C",
      thread: "store:clovis"
    });
    expect(item).toMatchObject({
      category: { path: "operations/food-safety", labels: ["Store operations", "Food safety"] },
      kind_label: "Temperature alarm",
      level: "critical",
      actions: [{ id: "ack", label: "Acknowledge", destructive: false, runs_function: true }],
      thread: threadKey("store:clovis")
    });
    // SHA-256, hex, the first 32 characters: opaque, and not the app's text.
    expect(item.thread).toMatch(/^[0-9a-f]{32}$/);
    expect(inbox.receive({ kind: "temp_alarm", title: "t" }).thread).toBeNull();
  });
});

describe("createTestInbox — filters", () => {
  function stocked() {
    const inbox = createTestInbox({ appId: "app-1", manifest: MANIFEST });
    inbox.receive({ kind: "temp_alarm", title: "alarm 1", thread: "store:clovis" });
    inbox.receive({ kind: "list_overdue", title: "late list" });
    inbox.receive({ kind: "temp_alarm", title: "alarm 2", thread: "store:fresno" });
    inbox.push({ title: "notice", app: null, kind: "oxy.billing_past_due" });
    return inbox;
  }
  const titles = async (inbox: ReturnType<typeof createTestInbox>, query: string) =>
    ((await get(inbox, `/api/notifications?unread_only=false&${query}`)) as Page).items.map(
      (i) => i.title
    );

  it("narrows a category to its subtree, with its source", async () => {
    const inbox = stocked();
    expect(await titles(inbox, "source=app-1&category=operations")).toEqual([
      "alarm 2",
      "late list",
      "alarm 1"
    ]);
    expect(await titles(inbox, "source=app-1&category=operations/food-safety")).toEqual([
      "alarm 2",
      "alarm 1"
    ]);
    expect(await titles(inbox, "source=oxygen")).toEqual(["notice"]);
  });

  it("refuses a category without a source, as the server does", async () => {
    expect((await stocked().fetcher("/api/notifications?category=operations")).status).toBe(400);
  });

  it("narrows by kind and by thread", async () => {
    const inbox = stocked();
    expect(await titles(inbox, "kind=list_overdue")).toEqual(["late list"]);
    expect(await titles(inbox, `thread=${threadKey("store:clovis")}`)).toEqual(["alarm 1"]);
  });

  it("counts and marks read only what the filter covers", async () => {
    const inbox = stocked();
    const unread = (query: string) => get(inbox, `/api/notifications/unread?${query}`);
    expect(await unread("app_id=app-1&kind=temp_alarm")).toEqual({ unread: 2 });
    await inbox.fetcher("/api/notifications/read-all?app_id=app-1&kind=temp_alarm", {
      method: "POST"
    });
    expect(await unread("app_id=app-1&kind=temp_alarm")).toEqual({ unread: 0 });
    expect(inbox.unread("app-1")).toBe(1);
  });

  it("pages with before, and says where the next page starts", async () => {
    const inbox = stocked();
    const first = (await get(inbox, "/api/notifications?unread_only=false&limit=2")) as Page;
    expect(first.items.map((i) => i.title)).toEqual(["notice", "alarm 2"]);
    const next = (await get(
      inbox,
      `/api/notifications?unread_only=false&limit=2&before=${first.next_before}`
    )) as Page;
    expect(next.items.map((i) => i.title)).toEqual(["late list", "alarm 1"]);
    expect(next.next_before).toBeNull();
  });
});
