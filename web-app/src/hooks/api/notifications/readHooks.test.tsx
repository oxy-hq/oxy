// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { AxiosError, type AxiosResponse } from "axios";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { InboxItem, NotificationCatalog } from "@/types/notifications";
import queryKeys from "../queryKey";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    catalog: vi.fn(),
    markRead: vi.fn(),
    markAllRead: vi.fn(),
    runAction: vi.fn()
  }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { useMarkAllNotificationsRead } from "./useMarkAllNotificationsRead";
import { useMarkNotificationRead } from "./useMarkNotificationRead";
import { useRunNotificationAction } from "./useRunNotificationAction";

const service = vi.mocked(NotificationsService);
const KEY = queryKeys.notifications.catalog();

const catalog = (): NotificationCatalog => ({
  reach: { push_devices: 0, email: true },
  sources: [
    {
      source: "app",
      app: { id: "app-1", slug: "store-ops", name: "Store Ops", org_slug: "acme" },
      label: "Store Ops",
      muted: [],
      unread: 3,
      categories: [
        {
          path: "operations",
          label: "Store operations",
          description: null,
          muted: [],
          unread: 3,
          categories: [],
          kinds: [
            {
              kind: "temp_alarm",
              label: "Temperature alarm",
              description: null,
              level: "critical",
              critical: true,
              channels: ["push", "email", "slack"],
              muted: [],
              actions: [],
              unread: 3
            }
          ]
        }
      ]
    }
  ]
});

const alarm: InboxItem = {
  id: "n-1",
  org_id: "org-1",
  kind: "temp_alarm",
  created_at: "2026-10-14T11:55:00Z",
  read_at: null,
  title: "Cooler 2 is warm",
  body: null,
  href: null,
  link: null,
  app: { id: "app-1", slug: "store-ops", name: "Store Ops" }
};

let qc: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={qc}>{children}</QueryClientProvider>
);

/** A catalogue in the cache, as a mounted page or Settings holds it. */
async function seeded() {
  service.catalog.mockResolvedValue(catalog());
  await qc.prefetchQuery({ queryKey: KEY, queryFn: () => NotificationsService.catalog() });
  service.catalog.mockClear();
}

const unread = () => qc.getQueryData<NotificationCatalog>(KEY)?.sources[0].unread;
const catalogueInvalidated = () => qc.getQueryState(KEY)?.isInvalidated ?? false;

beforeEach(() => {
  qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("useMarkNotificationRead", () => {
  it("drops the catalogue's count in the cache and does not re-read it", async () => {
    await seeded();
    service.markRead.mockResolvedValue();
    const { result } = renderHook(() => useMarkNotificationRead(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync(alarm);
    });
    expect(service.markRead).toHaveBeenCalledWith("n-1");
    expect(unread()).toBe(2);
    expect(catalogueInvalidated()).toBe(false);
    expect(service.catalog).not.toHaveBeenCalled();
  });

  it("re-reads the catalogue when the read failed", async () => {
    await seeded();
    service.markRead.mockRejectedValue(new Error("down"));
    const { result } = renderHook(() => useMarkNotificationRead(), { wrapper });
    act(() => result.current.mutate(alarm));
    await waitFor(() => expect(catalogueInvalidated()).toBe(true));
    expect(toast.error).toHaveBeenCalledWith("Couldn't mark that notification read");
  });
});

describe("useMarkAllNotificationsRead", () => {
  it("zeroes what the filter covers in the cache and does not re-read it", async () => {
    await seeded();
    service.markAllRead.mockResolvedValue();
    const { result } = renderHook(() => useMarkAllNotificationsRead(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({ source: "app-1", category: "operations" });
    });
    expect(unread()).toBe(0);
    expect(catalogueInvalidated()).toBe(false);
  });

  it("re-reads the catalogue for a thread, whose count it cannot tell", async () => {
    await seeded();
    service.markAllRead.mockResolvedValue();
    const { result } = renderHook(() => useMarkAllNotificationsRead(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({ source: "app-1", thread: "t-1" });
    });
    expect(unread()).toBe(3);
    expect(catalogueInvalidated()).toBe(true);
  });
});

describe("useRunNotificationAction", () => {
  it("drops the count once the function succeeded, with no re-read", async () => {
    await seeded();
    service.runAction.mockResolvedValue({ ok: true, result: { acked: true } });
    const { result } = renderHook(() => useRunNotificationAction(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({ item: alarm, actionId: "ack" });
    });
    expect(service.runAction).toHaveBeenCalledWith("n-1", "ack");
    expect(unread()).toBe(2);
    expect(catalogueInvalidated()).toBe(false);
    expect(toast.success).toHaveBeenCalledWith("Done");
  });

  it("leaves the count when the app's function refused, and says why", async () => {
    await seeded();
    service.runAction.mockRejectedValue(
      new AxiosError("Forbidden", "ERR_BAD_REQUEST", undefined, undefined, {
        status: 403,
        data: {
          ok: false,
          error: "FunctionRefused",
          message: "the app's function did not accept your sign-in"
        }
      } as AxiosResponse)
    );
    const { result } = renderHook(() => useRunNotificationAction(), { wrapper });
    act(() => result.current.mutate({ item: alarm, actionId: "ack" }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("the app's function did not accept your sign-in")
    );
    expect(unread()).toBe(3);
    expect(toast.success).not.toHaveBeenCalled();
  });
});
