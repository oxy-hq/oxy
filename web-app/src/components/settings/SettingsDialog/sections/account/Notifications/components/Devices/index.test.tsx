// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice } from "@/types/notifications";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    devices: vi.fn(),
    updateDevice: vi.fn(),
    removeDevice: vi.fn(),
    vapidPublicKey: vi.fn(),
    registerDevice: vi.fn(),
    settings: vi.fn(),
    catalog: vi.fn()
  }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const browser = vi.hoisted(() => ({ supported: false, homeScreen: false }));
vi.mock("@/libs/webPush", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/libs/webPush")>()),
  pushSupported: () => browser.supported,
  pushNeedsHomeScreenHere: () => browser.homeScreen,
  subscribeThisDevice: vi.fn().mockResolvedValue("https://push.example/sub/abc")
}));

import { rememberThisDevice } from "@/libs/webPush";
import { device } from "../../__tests__/fixtures";
import { Devices } from ".";

const service = vi.mocked(NotificationsService);

beforeAll(() => {
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});

const LAPTOP = device({ id: "d-1", name: "Work laptop" });
const IPHONE = device({
  id: "d-2",
  platform: "ios",
  name: "Luong's iPhone",
  os_permission: "denied"
});
const PIXEL = device({
  id: "d-3",
  platform: "android",
  name: null,
  min_level: "critical",
  os_disabled: ["app-1.temp_check", "oxygen.oxy.run_failed"]
});
const BLOCKED = device({ id: "d-4", name: "Old browser", os_permission: "denied", enabled: false });

/** Serve `devices`, and keep every write the way the server would. */
function fakeServer(initial: NotificationDevice[]) {
  let current = initial;
  service.devices.mockImplementation(async () => current);
  service.updateDevice.mockImplementation(async (id, patch) => {
    current = current.map((d) => (d.id === id ? { ...d, ...patch } : d));
  });
  service.removeDevice.mockImplementation(async (id) => {
    current = current.filter((d) => d.id !== id);
  });
  service.registerDevice.mockImplementation(async () => {
    current = [...current, device({ id: "d-9", name: "Chrome on macOS" })];
    return "d-9";
  });
}

const renderDevices = () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
  render(
    <QueryClientProvider client={queryClient}>
      <Devices />
    </QueryClientProvider>
  );
};

const part = (deviceId: string, name: string) =>
  screen.findByTestId(`notifications-device-${deviceId}-${name}`);
const row = (deviceId: string) => screen.findByTestId(`notifications-device-${deviceId}`);

beforeEach(() => {
  browser.supported = false;
  browser.homeScreen = false;
  localStorage.clear();
  fakeServer([LAPTOP, IPHONE, PIXEL, BLOCKED]);
  service.vapidPublicKey.mockResolvedValue({ configured: true, public_key: "BAAA" });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  Reflect.deleteProperty(window, "Notification");
});

describe("Devices list", () => {
  it("shows each device by name, or by what it is when unnamed", async () => {
    renderDevices();
    expect(await part("d-1", "name")).toHaveValue("Work laptop");
    expect(await part("d-2", "name")).toHaveValue("Luong's iPhone");
    expect(await part("d-3", "name")).toHaveValue("Android device");
    expect(await part("d-1", "seen")).toHaveTextContent(/^Seen /);
  });

  it("says what each device's own settings block", async () => {
    renderDevices();
    expect(await part("d-2", "status")).toHaveTextContent("Off in iOS Settings");
    expect(await part("d-3", "status")).toHaveTextContent("2 kinds off in Android Settings");
    expect(await part("d-4", "status")).toHaveTextContent("Blocked in browser settings");
    expect(screen.queryByTestId("notifications-device-d-1-status")).toBeNull();
  });

  it("marks the device this browser registered, and only it", async () => {
    rememberThisDevice("d-1");
    renderDevices();
    expect(await part("d-1", "this")).toHaveTextContent("This device");
    expect(screen.getAllByText("This device")).toHaveLength(1);
  });

  it("marks no device when this browser registered none", async () => {
    renderDevices();
    await row("d-1");
    expect(screen.queryByText("This device")).toBeNull();
  });

  it("says so when there are none", async () => {
    fakeServer([]);
    renderDevices();
    expect(await screen.findByTestId("notifications-devices-empty")).toHaveTextContent(
      "No devices yet"
    );
  });
});

describe("Devices writes", () => {
  it("renames a device on Enter", async () => {
    renderDevices();
    const name = await part("d-1", "name");
    await userEvent.clear(name);
    await userEvent.type(name, "Home laptop{Enter}");
    await waitFor(() =>
      expect(service.updateDevice).toHaveBeenCalledWith("d-1", { name: "Home laptop" })
    );
    expect(await part("d-1", "name")).toHaveValue("Home laptop");
  });

  it("puts the name back on Escape, or when emptied, and saves nothing", async () => {
    renderDevices();
    const name = await part("d-1", "name");
    await userEvent.type(name, " 2{Escape}");
    expect(name).toHaveValue("Work laptop");
    await userEvent.clear(name);
    await userEvent.tab();
    expect(name).toHaveValue("Work laptop");
    expect(service.updateDevice).not.toHaveBeenCalled();
  });

  it("switches push off on one device", async () => {
    renderDevices();
    const enabled = await part("d-1", "enabled");
    expect(enabled).toBeChecked();
    await userEvent.click(enabled);
    expect(await part("d-1", "enabled")).not.toBeChecked();
    expect(service.updateDevice).toHaveBeenCalledWith("d-1", { enabled: false });
  });

  it("sets and clears a device's floor", async () => {
    renderDevices();
    expect(await part("d-3", "floor")).toHaveTextContent("Critical only");
    await userEvent.click(await part("d-1", "floor"));
    await userEvent.click(await screen.findByRole("option", { name: "Time-sensitive and above" }));
    await waitFor(() =>
      expect(service.updateDevice).toHaveBeenCalledWith("d-1", { min_level: "time_sensitive" })
    );
    await userEvent.click(await part("d-3", "floor"));
    await userEvent.click(await screen.findByRole("option", { name: "Everything" }));
    await waitFor(() =>
      expect(service.updateDevice).toHaveBeenCalledWith("d-3", { min_level: null })
    );
    expect(await part("d-3", "floor")).toHaveTextContent("Everything");
  });

  it("removes a device", async () => {
    renderDevices();
    await userEvent.click(await part("d-2", "remove"));
    await waitFor(() => expect(screen.queryByTestId("notifications-device-d-2")).toBeNull());
    expect(service.removeDevice).toHaveBeenCalledWith("d-2");
  });

  it("puts a removed device back when the server refuses", async () => {
    service.removeDevice.mockRejectedValueOnce(new Error("boom"));
    renderDevices();
    await userEvent.click(await part("d-2", "remove"));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't remove that device"));
    expect(await row("d-2")).toBeInTheDocument();
  });

  it("reports what this browser allows when the row says otherwise", async () => {
    rememberThisDevice("d-1");
    Object.defineProperty(window, "Notification", {
      value: { permission: "denied" },
      configurable: true
    });
    renderDevices();
    await waitFor(() =>
      expect(service.updateDevice).toHaveBeenCalledWith("d-1", { os_permission: "denied" })
    );
    expect(await part("d-1", "status")).toHaveTextContent("Blocked in browser settings");
  });

  it("reports nothing when the row already says the same", async () => {
    rememberThisDevice("d-1");
    Object.defineProperty(window, "Notification", {
      value: { permission: "granted" },
      configurable: true
    });
    renderDevices();
    await row("d-1");
    expect(service.updateDevice).not.toHaveBeenCalled();
  });
});

describe("Push on this device", () => {
  it("is not available in a browser without push", async () => {
    renderDevices();
    expect(await screen.findByTestId("notifications-push-unavailable")).toHaveTextContent(
      "Not available here"
    );
    expect(screen.queryByTestId("notifications-push-home-screen")).toBeNull();
  });

  it("is not available where the deployment has no push key", async () => {
    browser.supported = true;
    service.vapidPublicKey.mockResolvedValue({ configured: false, public_key: null });
    renderDevices();
    expect(await screen.findByTestId("notifications-push-unavailable")).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-push-enable")).toBeNull();
  });

  it("on an iPhone in a browser tab, says to add Oxygen to the Home Screen", async () => {
    browser.homeScreen = true;
    renderDevices();
    expect(await screen.findByTestId("notifications-push-home-screen")).toHaveTextContent(
      "Add Oxygen to your Home Screen (Share → Add to Home Screen) to get notifications here."
    );
    expect(screen.queryByTestId("notifications-push-unavailable")).toBeNull();
    expect(screen.queryByTestId("notifications-push-enable")).toBeNull();
  });

  it("has no Home Screen hint where the deployment has no push key", async () => {
    browser.homeScreen = true;
    service.vapidPublicKey.mockResolvedValue({ configured: false, public_key: null });
    renderDevices();
    expect(await screen.findByTestId("notifications-push-unavailable")).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-push-home-screen")).toBeNull();
  });

  it("registers this browser, then lists it as this device", async () => {
    browser.supported = true;
    renderDevices();
    await userEvent.click(await screen.findByTestId("notifications-push-enable"));
    await waitFor(() =>
      expect(service.registerDevice).toHaveBeenCalledWith(
        expect.objectContaining({ platform: "web", token: "https://push.example/sub/abc" })
      )
    );
    expect(await screen.findByTestId("notifications-push-enable")).toHaveTextContent("On");
    expect(await part("d-9", "this")).toHaveTextContent("This device");
  });
});
