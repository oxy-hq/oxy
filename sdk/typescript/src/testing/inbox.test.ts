// `createTestInbox` answers the inbox routes as the server does — the app
// filter, the unread-only default, read state — and takes a function test's
// `ctx.notify` payload as the item its recipient sees.

import { describe, expect, it } from "vitest";
import { createTestContext } from "./context";
import { createTestInbox } from "./inbox";

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

  it("acts as an older server: ignores app_id and sends no link", async () => {
    const inbox = createTestInbox({ appId: "app-1", legacy: true });
    inbox.push({ link: "/a" });
    inbox.push({ app: { id: "app-2", slug: "x", name: "X" } });
    const page = (await get(inbox, "/api/notifications?app_id=app-1")) as {
      items: Record<string, unknown>[];
    };
    expect(page.items).toHaveLength(2);
    expect(page.items.every((i) => !("link" in i))).toBe(true);
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
