// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { device } from "@/components/settings/SettingsDialog/sections/account/Notifications/__tests__/fixtures";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice } from "@/types/notifications";
import queryKeys from "../queryKey";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    devices: vi.fn(),
    updateDevice: vi.fn(),
    removeDevice: vi.fn(),
    registerDevice: vi.fn(),
    settings: vi.fn(),
    catalog: vi.fn()
  }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("@/libs/webPush", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/libs/webPush")>()),
  requestPushPermission: vi.fn().mockResolvedValue("granted"),
  subscribeThisDevice: vi.fn().mockResolvedValue({
    endpoint: "https://push.example/sub/abc",
    keys: { p256dh: "BPub", auth: "c2VjcmV0" }
  })
}));

import { rememberThisDevice, thisDeviceId } from "@/libs/webPush";
import { useEnablePushOnThisDevice } from "./useEnablePushOnThisDevice";
import { useRemoveNotificationDevice } from "./useRemoveNotificationDevice";
import { useUpdateNotificationDevice } from "./useUpdateNotificationDevice";

const service = vi.mocked(NotificationsService);
const KEY = queryKeys.notifications.devices();
const LAPTOP = device({ id: "d-1", name: "Work laptop" });
const PHONE = device({ id: "d-2", platform: "ios", name: "Luong's iPhone" });

function setup<T>(hook: () => T, devices: NotificationDevice[] = [LAPTOP, PHONE]) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
  qc.setQueryData(KEY, devices);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  const { result } = renderHook(hook, { wrapper });
  return { result, cached: () => qc.getQueryData<NotificationDevice[]>(KEY) };
}

beforeEach(() => {
  // A refetch after a write answers with what the cache started with.
  service.devices.mockResolvedValue([LAPTOP, PHONE]);
  localStorage.clear();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("useUpdateNotificationDevice", () => {
  it("changes the row at once", async () => {
    let finish!: () => void;
    service.updateDevice.mockReturnValue(new Promise((res) => (finish = () => res())));
    const { result, cached } = setup(() => useUpdateNotificationDevice());

    act(() =>
      result.current.mutate({ id: "d-2", patch: { enabled: false, min_level: "critical" } })
    );
    await waitFor(() =>
      expect(cached()?.find((d) => d.id === "d-2")).toMatchObject({
        enabled: false,
        min_level: "critical"
      })
    );
    expect(service.updateDevice).toHaveBeenCalledWith("d-2", {
      enabled: false,
      min_level: "critical"
    });
    act(() => finish());
  });

  it("puts the row back when the server refuses, and says so", async () => {
    service.updateDevice.mockRejectedValue(new Error("boom"));
    service.devices.mockReturnValue(new Promise(() => {}));
    const { result, cached } = setup(() => useUpdateNotificationDevice());

    act(() => result.current.mutate({ id: "d-1", patch: { name: "Home laptop" } }));
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(cached()).toEqual([LAPTOP, PHONE]);
    expect(toast.error).toHaveBeenCalledWith("Couldn't update that device");
  });
});

describe("useRemoveNotificationDevice", () => {
  it("drops the row at once and forgets it as this device", async () => {
    rememberThisDevice("d-1");
    service.removeDevice.mockResolvedValue();
    service.devices.mockReturnValue(new Promise(() => {}));
    const { result, cached } = setup(() => useRemoveNotificationDevice());

    act(() => result.current.mutate("d-1"));
    await waitFor(() => expect(cached()).toEqual([PHONE]));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(thisDeviceId()).toBeNull();
  });

  it("puts the row back in its place when the server refuses", async () => {
    rememberThisDevice("d-1");
    service.removeDevice.mockRejectedValue(new Error("boom"));
    service.devices.mockReturnValue(new Promise(() => {}));
    const { result, cached } = setup(() => useRemoveNotificationDevice());

    act(() => result.current.mutate("d-1"));
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(cached()).toEqual([LAPTOP, PHONE]);
    expect(thisDeviceId()).toBe("d-1");
    expect(toast.error).toHaveBeenCalledWith("Couldn't remove that device");
  });
});

describe("useEnablePushOnThisDevice", () => {
  it("registers this browser by name, with its keys, and remembers the id the server answers with", async () => {
    service.registerDevice.mockResolvedValue("d-9");
    const { result } = setup(() => useEnablePushOnThisDevice(), []);

    act(() => result.current.turnOn("BKEY"));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(service.registerDevice).toHaveBeenCalledWith({
      platform: "web",
      token: "https://push.example/sub/abc",
      device_name: expect.any(String),
      keys: { p256dh: "BPub", auth: "c2VjcmV0" }
    });
    expect(thisDeviceId()).toBe("d-9");
  });

  it("sends back the name the person gave this browser rather than replacing it", async () => {
    rememberThisDevice("d-1");
    service.registerDevice.mockResolvedValue("d-1");
    const { result } = setup(() => useEnablePushOnThisDevice());

    act(() => result.current.turnOn("BKEY"));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(service.registerDevice).toHaveBeenCalledWith(
      expect.objectContaining({ device_name: "Work laptop" })
    );
  });

  it("remembers nothing from a server that answers without an id", async () => {
    service.registerDevice.mockResolvedValue(null);
    const { result } = setup(() => useEnablePushOnThisDevice(), []);

    act(() => result.current.turnOn("BKEY"));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(thisDeviceId()).toBeNull();
  });
});
