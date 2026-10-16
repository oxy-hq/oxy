// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { KindNode, NotificationCatalog } from "@/types/notifications";
import queryKeys from "../queryKey";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: { setPreference: vi.fn(), catalog: vi.fn() }
}));

import { useSetNotificationPreference, withPreference } from "./useSetNotificationPreference";

const KEY = queryKeys.notifications.catalog();

const kind = (name: string, muted: KindNode["muted"] = []): KindNode => ({
  kind: name,
  label: name,
  description: null,
  level: "active",
  critical: false,
  channels: ["push", "email", "slack"],
  muted,
  actions: [],
  unread: 0
});

const catalog = (): NotificationCatalog => ({
  reach: { push_devices: 1, email: true },
  sources: [
    {
      source: "oxygen",
      app: null,
      label: "Oxygen",
      muted: [],
      unread: 0,
      categories: [
        {
          path: "runs",
          label: "Automations and pipelines",
          description: null,
          muted: [],
          unread: 0,
          categories: [],
          kinds: [kind("oxy.run_failed")]
        }
      ]
    },
    {
      source: "app",
      app: { id: "app-1", slug: "ops", name: "Ops", org_slug: "acme" },
      label: "Ops",
      muted: [],
      unread: 0,
      categories: [
        {
          path: "operations",
          label: "Store operations",
          description: null,
          muted: ["email"],
          unread: 0,
          kinds: [kind("list_overdue", ["email"]), kind("shift_swap")],
          categories: [
            {
              path: "operations/food-safety",
              label: "Food safety",
              description: null,
              muted: [],
              unread: 0,
              categories: [],
              kinds: [kind("list_overdue")]
            }
          ]
        }
      ]
    },
    {
      source: "app",
      app: { id: "app-2", slug: "books", name: "Books", org_slug: "acme" },
      label: "Books",
      muted: [],
      unread: 0,
      categories: [
        {
          path: "general",
          label: "General",
          description: null,
          muted: [],
          unread: 0,
          categories: [],
          kinds: [kind("list_overdue")]
        }
      ]
    }
  ]
});

const ops = (c: NotificationCatalog) => c.sources[1];

afterEach(() => {
  vi.clearAllMocks();
});

describe("withPreference", () => {
  it("mutes one channel of one kind of one app, wherever the kind sits", () => {
    const next = withPreference(catalog(), {
      app_id: "app-1",
      kind: "list_overdue",
      channel: "push",
      muted: true
    });
    expect(ops(next).categories[0].kinds[0].muted).toEqual(["email", "push"]);
    expect(ops(next).categories[0].categories[0].kinds[0].muted).toEqual(["push"]);
    // The same kind in another app, and another kind in this one, stay as they were.
    expect(next.sources[2]).toEqual(catalog().sources[2]);
    expect(ops(next).categories[0].kinds[1]).toEqual(ops(catalog()).categories[0].kinds[1]);
  });

  it("mutes a whole source with scope ''", () => {
    const next = withPreference(catalog(), {
      app_id: "app-1",
      scope: "",
      channel: "slack",
      muted: true
    });
    expect(ops(next).muted).toEqual(["slack"]);
    expect(ops(next).categories).toEqual(ops(catalog()).categories);
  });

  it("mutes a category by its path, and only that category", () => {
    const next = withPreference(catalog(), {
      app_id: "app-1",
      scope: "operations/food-safety",
      channel: "push",
      muted: true
    });
    expect(ops(next).categories[0].categories[0].muted).toEqual(["push"]);
    expect(ops(next).categories[0].muted).toEqual(["email"]);
    expect(ops(next).muted).toEqual([]);
  });

  it("unmutes, and muting twice does not list a channel twice", () => {
    const off = withPreference(catalog(), {
      app_id: "app-1",
      scope: "operations",
      channel: "email",
      muted: false
    });
    expect(ops(off).categories[0].muted).toEqual([]);
    const again = withPreference(catalog(), {
      app_id: "app-1",
      scope: "operations",
      channel: "email",
      muted: true
    });
    expect(ops(again).categories[0].muted).toEqual(["email"]);
  });

  it("moves Oxygen's tree when app_id is null, and leaves every app alone", () => {
    const next = withPreference(catalog(), {
      app_id: null,
      scope: "runs",
      channel: "email",
      muted: true
    });
    expect(next.sources[0].categories[0].muted).toEqual(["email"]);
    expect(next.sources.slice(1)).toEqual(catalog().sources.slice(1));
  });
});

describe("useSetNotificationPreference", () => {
  const mount = () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
    });
    queryClient.setQueryData(KEY, catalog());
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useSetNotificationPreference(), { wrapper });
    const cached = () => queryClient.getQueryData<NotificationCatalog>(KEY);
    return { result, cached };
  };

  it("sends the scope form as given, and moves the switch before the server answers", async () => {
    let answer!: () => void;
    vi.mocked(NotificationsService.setPreference).mockReturnValue(
      new Promise<void>((resolve) => {
        answer = resolve;
      })
    );
    const { result, cached } = mount();
    act(() => {
      result.current.mutate({ app_id: "app-1", scope: "", channel: "slack", muted: true });
    });
    await waitFor(() =>
      expect(NotificationsService.setPreference).toHaveBeenCalledWith({
        app_id: "app-1",
        scope: "",
        channel: "slack",
        muted: true
      })
    );
    expect(cached()?.sources[1].muted).toEqual(["slack"]);
    await act(async () => answer());
  });

  it("puts the switch back when the server refuses", async () => {
    vi.mocked(NotificationsService.setPreference).mockRejectedValue(new Error("refused"));
    const { result, cached } = mount();
    act(() => {
      result.current.mutate({ app_id: "app-1", kind: "shift_swap", channel: "slack", muted: true });
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(cached()?.sources[1].categories[0].kinds[1].muted).toEqual([]);
  });
});
