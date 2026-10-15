// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationPreferences } from "@/types/notifications";
import queryKeys from "../queryKey";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: { setPreference: vi.fn(), preferences: vi.fn() }
}));

import { useSetNotificationPreference, withPreference } from "./useSetNotificationPreference";

const KEY = queryKeys.notifications.preferences();

const prefs = (): NotificationPreferences => ({
  apps: [
    {
      app: { id: "app-1", slug: "ops", name: "Ops" },
      kinds: [
        { kind: "list_overdue", muted: ["email"], critical: false },
        { kind: "shift_swap", muted: [], critical: false }
      ]
    },
    {
      app: { id: "app-2", slug: "books", name: "Books" },
      kinds: [{ kind: "list_overdue", muted: [], critical: false }]
    }
  ],
  platform: [{ kind: "oxy.run_failed", muted: [], critical: false }],
  reach: { push_devices: 1, email: true }
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("withPreference", () => {
  it("mutes one channel of one kind of one app", () => {
    const next = withPreference(prefs(), {
      app_id: "app-1",
      kind: "list_overdue",
      channel: "push",
      muted: true
    });
    expect(next.apps[0].kinds[0].muted).toEqual(["email", "push"]);
    // The same kind in another app, and another kind in this one, stay as they were.
    expect(next.apps[1]).toEqual(prefs().apps[1]);
    expect(next.apps[0].kinds[1]).toEqual(prefs().apps[0].kinds[1]);
  });

  it("unmutes, and muting twice does not list a channel twice", () => {
    const base = prefs();
    const unmuted = withPreference(base, {
      app_id: "app-1",
      kind: "list_overdue",
      channel: "email",
      muted: false
    });
    expect(unmuted.apps[0].kinds[0].muted).toEqual([]);
    const again = withPreference(base, {
      app_id: "app-1",
      kind: "list_overdue",
      channel: "email",
      muted: true
    });
    expect(again.apps[0].kinds[0].muted).toEqual(["email"]);
  });

  it("mutes an Oxygen kind when app_id is null, and leaves every app alone", () => {
    const next = withPreference(prefs(), {
      app_id: null,
      kind: "oxy.run_failed",
      channel: "email",
      muted: true
    });
    expect(next.platform[0].muted).toEqual(["email"]);
    expect(next.apps).toEqual(prefs().apps);
  });

  it("leaves Oxygen's kinds alone when an app's kind moves", () => {
    const next = withPreference(prefs(), {
      app_id: "app-1",
      kind: "oxy.run_failed",
      channel: "email",
      muted: true
    });
    expect(next.platform).toEqual(prefs().platform);
  });
});

describe("useSetNotificationPreference", () => {
  const mount = () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
    });
    queryClient.setQueryData(KEY, prefs());
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useSetNotificationPreference(), { wrapper });
    const cached = () => queryClient.getQueryData<NotificationPreferences>(KEY);
    return { result, cached };
  };

  it("moves the switch before the server answers", async () => {
    let answer!: () => void;
    vi.mocked(NotificationsService.setPreference).mockReturnValue(
      new Promise<void>((resolve) => {
        answer = resolve;
      })
    );
    const { result, cached } = mount();
    act(() => {
      result.current.mutate({ app_id: "app-1", kind: "shift_swap", channel: "slack", muted: true });
    });
    await waitFor(() => expect(cached()?.apps[0].kinds[1].muted).toEqual(["slack"]));
    await act(async () => answer());
  });

  it("puts the switch back when the server refuses", async () => {
    vi.mocked(NotificationsService.setPreference).mockRejectedValue(new Error("refused"));
    const { result, cached } = mount();
    act(() => {
      result.current.mutate({ app_id: "app-1", kind: "shift_swap", channel: "slack", muted: true });
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(cached()?.apps[0].kinds[1].muted).toEqual([]);
  });
});
