// @vitest-environment jsdom
//
// `<NotificationsBell />` mounted for real against the fake inbox: the badge,
// opening and closing, where a tapped notification goes, and the empty state.

import * as React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestInbox, type TestInbox } from "../testing/inbox";
import { pageNavigation } from "./notifications";
import { APP_ID, click, mount, type Page, servePage, settle } from "./notifications.harness";
import { NotificationsBell, type NotificationsBellProps } from "./notifications-bell";

let page: Page | undefined;

async function render(inbox: TestInbox, props: NotificationsBellProps = {}): Promise<Page> {
  page = await mount(<NotificationsBell {...props} />, inbox.fetcher);
  return page;
}

const $ = (id: string) => document.querySelector(`[data-testid="${id}"]`);
const bell = () => $("oxy-notifications-bell") as HTMLButtonElement | null;
const panel = () => $("oxy-notifications-panel") as HTMLDivElement | null;

async function openPanel(): Promise<void> {
  await click(bell());
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  servePage();
  window.history.replaceState(null, "", "/customer-apps/acme/bookings/today");
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
});

afterEach(async () => {
  await page?.unmount();
  page = undefined;
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("NotificationsBell — the badge", () => {
  it("shows the unread count and says it", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push();
    inbox.push();
    inbox.push({ read_at: "2026-01-01T00:00:00Z" });
    await render(inbox);
    expect($("oxy-notifications-badge")?.textContent).toBe("2");
    expect(bell()?.getAttribute("aria-label")).toBe("Notifications, 2 unread");
  });

  it("has no badge at zero", async () => {
    await render(createTestInbox({ appId: APP_ID }));
    expect(bell()).not.toBeNull();
    expect($("oxy-notifications-badge")).toBeNull();
  });

  it("renders nothing where there is no inbox", async () => {
    const p = await render(createTestInbox({ appId: APP_ID, status: 404 }));
    expect(p.container.innerHTML).toBe("");
  });
});

describe("NotificationsBell — open and close", () => {
  it("opens on click and fetches the list then", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push({ title: "New booking", body: "Table for 4" });
    await render(inbox);
    expect(inbox.requests.some((r) => r.path === "/api/notifications")).toBe(false);
    await openPanel();
    expect(panel()?.getAttribute("role")).toBe("dialog");
    expect(bell()?.getAttribute("aria-expanded")).toBe("true");
    expect(panel()?.textContent).toContain("New booking");
    expect(panel()?.textContent).toContain("Table for 4");
  });

  it("closes on the bell, on Escape (focus back to the bell), and on a press outside", async () => {
    await render(createTestInbox({ appId: APP_ID }));
    await openPanel();
    await click(bell());
    expect(panel()).toBeNull();

    await openPanel();
    await React.act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(panel()).toBeNull();
    expect(document.activeElement).toBe(bell());

    await openPanel();
    await React.act(async () => {
      document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(panel()).toBeNull();
  });

  it("says so when there is nothing", async () => {
    await render(createTestInbox({ appId: APP_ID }));
    await openPanel();
    expect($("oxy-notifications-empty")?.textContent).toBe("You're all caught up");
    expect(($("oxy-notifications-mark-all-read") as HTMLButtonElement).disabled).toBe(true);
  });

  it("marks all read from the header", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    inbox.push();
    await render(inbox);
    await openPanel();
    await click($("oxy-notifications-mark-all-read"));
    expect(inbox.unread(APP_ID)).toBe(0);
    expect($("oxy-notifications-badge")).toBeNull();
  });

  it("spans the screen on a phone", async () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 375 });
    await render(createTestInbox({ appId: APP_ID }));
    await openPanel();
    expect(panel()?.style.left).toBe("8px");
    expect(panel()?.style.right).toBe("8px");
    expect(panel()?.style.width).toBe("");
  });
});

describe("NotificationsBell — following a notification", () => {
  it("hands this app's link to onNavigate, marks it read, and closes", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    const n = inbox.push({ link: "/bookings/1" });
    const onNavigate = vi.fn();
    const assign = vi.spyOn(pageNavigation, "assign").mockImplementation(() => {});
    await render(inbox, { onNavigate });
    await openPanel();
    await click($(`oxy-notifications-item-${n.id}`));
    expect(onNavigate).toHaveBeenCalledWith("/bookings/1", expect.objectContaining({ id: n.id }));
    expect(assign).not.toHaveBeenCalled();
    expect(inbox.items[0].read_at).not.toBeNull();
    expect(panel()).toBeNull();
  });

  it("navigates under the app's base path without onNavigate", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    const n = inbox.push({ link: "/bookings/1" });
    const assign = vi.spyOn(pageNavigation, "assign").mockImplementation(() => {});
    await render(inbox);
    await openPanel();
    await click($(`oxy-notifications-item-${n.id}`));
    expect(assign).toHaveBeenCalledWith("/customer-apps/acme/bookings/bookings/1");
  });

  it("opens another app's in its own tab and a platform notice on Oxygen", async () => {
    const inbox = createTestInbox({ appId: APP_ID, orgSlug: "acme" });
    const other = inbox.push({ app: { id: "app-2", slug: "lists", name: "Lists" }, link: "/l/9" });
    const notice = inbox.push({ app: null, link: null, href: "/settings/billing" });
    const open = vi.spyOn(pageNavigation, "open").mockImplementation(() => {});
    await render(inbox, { scope: "all", productBaseUrl: "https://app.oxygen-hq.com" });
    await openPanel();
    expect(panel()?.textContent).toContain("Lists");
    expect(panel()?.textContent).toContain("Oxygen");
    await click($(`oxy-notifications-item-${other.id}`));
    expect(open).toHaveBeenLastCalledWith(
      "https://app.oxygen-hq.com/customer-apps/acme/lists/l/9",
      "oxy-app-acme-lists"
    );
    await openPanel();
    await click($(`oxy-notifications-item-${notice.id}`));
    expect(open).toHaveBeenLastCalledWith(
      "https://app.oxygen-hq.com/settings/billing",
      "_blank",
      "noopener,noreferrer"
    );
  });

  it("only marks read, and stays open, when there is nowhere to go", async () => {
    const inbox = createTestInbox({ appId: APP_ID });
    const n = inbox.push({ link: null });
    const assign = vi.spyOn(pageNavigation, "assign").mockImplementation(() => {});
    await render(inbox);
    await openPanel();
    await click($(`oxy-notifications-item-${n.id}`));
    expect(assign).not.toHaveBeenCalled();
    expect(inbox.items[0].read_at).not.toBeNull();
    expect(panel()).not.toBeNull();
  });
});

describe("NotificationsBell — a staging or dev host", () => {
  it("offers no Mark all read, and a tap opens without marking read", async () => {
    servePage(APP_ID, "staging");
    const inbox = createTestInbox({ appId: APP_ID });
    const n = inbox.push({ link: "/bookings/1" });
    const onNavigate = vi.fn();
    await render(inbox, { onNavigate });
    await openPanel();
    expect(panel()).not.toBeNull();
    expect($("oxy-notifications-mark-all-read")).toBeNull();
    await click($(`oxy-notifications-item-${n.id}`));
    expect(onNavigate).toHaveBeenCalledWith("/bookings/1", expect.objectContaining({ id: n.id }));
    expect(inbox.requests.filter((r) => r.method === "POST")).toEqual([]);
    expect(inbox.items[0].read_at).toBeNull();
  });
});
