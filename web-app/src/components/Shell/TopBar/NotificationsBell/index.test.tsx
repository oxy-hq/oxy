// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import queryKeys from "@/hooks/api/queryKey";
import { NotificationsService } from "@/services/api/notifications";
import type { InboxItem } from "@/types/notifications";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    unread: vi.fn(),
    inbox: vi.fn(),
    markRead: vi.fn(),
    markAllRead: vi.fn()
  }
}));
vi.mock("@/contexts/AuthContext", () => ({ useAuth: () => ({ isLocalMode: false }) }));

import { NotificationsBell } from ".";

const service = vi.mocked(NotificationsService);

const NOW = new Date("2026-10-14T12:00:00Z");

const item = (over: Partial<InboxItem> = {}): InboxItem => ({
  id: "n-app",
  org_id: "org-1",
  kind: "list_overdue",
  created_at: "2026-10-14T11:55:00Z",
  read_at: null,
  title: "Walk-in cooler list is overdue",
  body: "3 of 12 checks are still open.",
  href: "/customer-apps/acme/store-ops/lists/42",
  app: { id: "app-1", slug: "store-ops", name: "Store Ops" },
  ...over
});

const platformItem = item({
  id: "n-platform",
  kind: "workspace_ready",
  title: "Your workspace is ready",
  body: null,
  href: "/acme/workspaces/ws-1/threads",
  app: null,
  read_at: "2026-10-14T11:00:00Z"
});

const unreadableItem = item({ id: "n-unreadable", title: null, body: null, href: null });

function Where() {
  const location = useLocation();
  return <p data-testid='where'>{`${location.pathname}${location.search}`}</p>;
}

function renderBell(url = "/acme/workspaces/ws-1/home") {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[url]}>
        <Routes>
          <Route
            path='*'
            element={
              <>
                <NotificationsBell />
                <Where />
              </>
            }
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
  return queryClient;
}

const openInbox = async () => {
  await userEvent.click(screen.getByTestId("notifications-bell"));
  return screen.findByTestId("notifications-panel");
};

// jsdom cannot navigate a document; the anchors' default action is not what is
// under test, only where they point and what the click does first.
const stopNavigation = (event: MouseEvent) => event.preventDefault();

beforeEach(() => {
  // Only `Date` is faked: React Query and Testing Library keep real timers.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
  document.addEventListener("click", stopNavigation);
  service.unread.mockResolvedValue({ unread: 2 });
  service.inbox.mockResolvedValue({ unread: 2, items: [item(), platformItem, unreadableItem] });
  service.markRead.mockResolvedValue();
  service.markAllRead.mockResolvedValue();
});

afterEach(() => {
  cleanup();
  document.removeEventListener("click", stopNavigation);
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("NotificationsBell badge", () => {
  it("is hidden when nothing is unread", async () => {
    service.unread.mockResolvedValue({ unread: 0 });
    const queryClient = renderBell();
    await waitFor(() =>
      expect(queryClient.getQueryState(queryKeys.notifications.unread())?.status).toBe("success")
    );
    expect(screen.queryByTestId("notifications-badge")).toBeNull();
    expect(screen.getByTestId("notifications-bell")).toHaveAccessibleName("Notifications");
  });

  it("shows the unread count", async () => {
    service.unread.mockResolvedValue({ unread: 7 });
    renderBell();
    expect(await screen.findByTestId("notifications-badge")).toHaveTextContent("7");
    expect(screen.getByTestId("notifications-bell")).toHaveAccessibleName(
      "Notifications, 7 unread"
    );
  });

  it("caps the count at 99+", async () => {
    service.unread.mockResolvedValue({ unread: 150 });
    renderBell();
    expect(await screen.findByTestId("notifications-badge")).toHaveTextContent("99+");
  });
});

describe("NotificationsBell inbox", () => {
  it("lists every notification, read and unread, with where it came from and when", async () => {
    renderBell();
    const panel = await openInbox();
    const appRow = await within(panel).findByTestId("notifications-item-n-app");
    expect(appRow).toHaveTextContent("Store Ops");
    expect(appRow).toHaveTextContent("Walk-in cooler list is overdue");
    expect(appRow).toHaveTextContent("3 of 12 checks are still open.");
    expect(appRow).toHaveTextContent("5 minutes ago");
    expect(appRow).toHaveAttribute("data-unread", "true");

    const platformRow = within(panel).getByTestId("notifications-item-n-platform");
    expect(platformRow).toHaveTextContent("Oxygen");
    expect(platformRow).toHaveAttribute("data-unread", "false");
    expect(service.inbox).toHaveBeenCalledWith({ unreadOnly: false, limit: 50 });
  });

  it("says so when a notification's content could not be read", async () => {
    renderBell();
    const panel = await openInbox();
    const row = await within(panel).findByTestId("notifications-item-n-unreadable");
    expect(row).toHaveTextContent("Couldn't load this notification");
  });

  it("opens an app's notification in that app's own tab, and marks it read", async () => {
    renderBell();
    const panel = await openInbox();
    const row = await within(panel).findByTestId("notifications-item-n-app");
    expect(row).toHaveAttribute("href", "/customer-apps/acme/store-ops/lists/42");
    expect(row).toHaveAttribute("target", "oxy-app-acme-store-ops");
    // A named tab is reused only without `rel=noopener` — see appWindowName.
    expect(row).not.toHaveAttribute("rel");

    await userEvent.click(row);
    await waitFor(() => expect(service.markRead).toHaveBeenCalledWith("n-app"));
    // The app opened elsewhere; this tab stays where it was.
    expect(screen.getByTestId("where")).toHaveTextContent("/acme/workspaces/ws-1/home");
  });

  it("follows a platform notice inside the product, without re-marking it read", async () => {
    renderBell();
    const panel = await openInbox();
    await userEvent.click(await within(panel).findByTestId("notifications-item-n-platform"));
    expect(screen.getByTestId("where")).toHaveTextContent("/acme/workspaces/ws-1/threads");
    await waitFor(() => expect(screen.queryByTestId("notifications-panel")).toBeNull());
    expect(service.markRead).not.toHaveBeenCalled();
  });

  it("marks everything read", async () => {
    renderBell();
    const panel = await openInbox();
    const button = within(panel).getByTestId("notifications-mark-all-read");
    await waitFor(() => expect(button).toBeEnabled());
    await userEvent.click(button);
    await waitFor(() => expect(service.markAllRead).toHaveBeenCalledTimes(1));
  });

  it("has nothing to mark when everything is read", async () => {
    service.inbox.mockResolvedValue({ unread: 0, items: [platformItem] });
    renderBell();
    const panel = await openInbox();
    await within(panel).findByTestId("notifications-item-n-platform");
    expect(within(panel).getByTestId("notifications-mark-all-read")).toBeDisabled();
  });

  it("shows one line when the inbox is empty", async () => {
    service.inbox.mockResolvedValue({ unread: 0, items: [] });
    renderBell();
    const panel = await openInbox();
    expect(await within(panel).findByTestId("notifications-empty")).toBeInTheDocument();
  });
});

describe("NotificationsBell opened from outside", () => {
  it("opens for ?notifications=open, and drops the param", async () => {
    renderBell("/acme/workspaces/ws-1/home?notifications=open&tab=x");
    expect(await screen.findByTestId("notifications-panel")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("where")).toHaveTextContent(
        /^\/acme\/workspaces\/ws-1\/home\?tab=x$/
      )
    );
  });

  it("opens when the push worker says its notification was clicked", async () => {
    const worker = new EventTarget();
    Object.defineProperty(navigator, "serviceWorker", { value: worker, configurable: true });
    try {
      renderBell();
      expect(screen.queryByTestId("notifications-panel")).toBeNull();
      act(() => {
        worker.dispatchEvent(
          new MessageEvent("message", { data: { type: "oxy:open-notifications" } })
        );
      });
      expect(await screen.findByTestId("notifications-panel")).toBeInTheDocument();
    } finally {
      Reflect.deleteProperty(navigator, "serviceWorker");
    }
  });
});
