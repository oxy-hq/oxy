// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HOME_SCREEN_HINT, thisDeviceId } from "@/libs/webPush";
import { NotificationsService } from "@/services/api/notifications";
import { FAKE_SUBSCRIPTION, fakePushBrowser, noPushBrowser } from "./__tests__/fakePushBrowser";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    vapidPublicKey: vi.fn(),
    registerDevice: vi.fn(),
    catalog: vi.fn()
  }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
// The real library, but for the iPhone check: jsdom has no `matchMedia` to ask.
const device = vi.hoisted(() => ({ homeScreen: false }));
vi.mock("@/libs/webPush", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/libs/webPush")>()),
  pushNeedsHomeScreenHere: () => device.homeScreen
}));

import { PushPrompt } from ".";
import { DISMISS_FOR_MS, DISMISSED_AT_KEY, type PushPromptInputs, pushPromptState } from "./utils";

const service = vi.mocked(NotificationsService);
const PUBLIC_KEY = `B${"A".repeat(86)}`;
const NOW = new Date("2026-10-17T12:00:00Z").getTime();

function renderPrompts(count = 1) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      {Array.from({ length: count }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list of identical prompts
        <PushPrompt key={i} />
      ))}
    </QueryClientProvider>
  );
}

/** Settle every pending promise and effect, so "not shown" is a verdict, not a race. */
async function settled() {
  await waitFor(() => expect(service.vapidPublicKey).toHaveBeenCalled());
  await act(async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  });
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
  window.localStorage.clear();
  device.homeScreen = false;
  service.vapidPublicKey.mockResolvedValue({ configured: true, public_key: PUBLIC_KEY });
  service.registerDevice.mockResolvedValue("d-9");
  service.catalog.mockResolvedValue({ sources: [] } as never);
});

afterEach(() => {
  cleanup();
  noPushBrowser();
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("pushPromptState", () => {
  const ready: PushPromptInputs = {
    configured: true,
    supported: true,
    needsHomeScreen: false,
    permission: "default",
    subscribed: false,
    dismissed: false
  };

  it("offers push only when every condition holds", () => {
    expect(pushPromptState(ready)).toBe("offer");
    const hidden: Partial<PushPromptInputs>[] = [
      { configured: false },
      { supported: false },
      { permission: "granted" },
      { permission: "denied" },
      { permission: "unsupported" },
      { subscribed: true },
      { subscribed: undefined },
      { dismissed: true }
    ];
    for (const change of hidden) {
      expect(pushPromptState({ ...ready, ...change }), JSON.stringify(change)).toBe("hidden");
    }
  });

  it("gives an iPhone tab the Home Screen hint, unless push is off or dismissed", () => {
    const iPhone = { ...ready, needsHomeScreen: true, supported: false, permission: "unsupported" };
    expect(pushPromptState(iPhone as PushPromptInputs)).toBe("home-screen");
    expect(pushPromptState({ ...(iPhone as PushPromptInputs), configured: false })).toBe("hidden");
    expect(pushPromptState({ ...(iPhone as PushPromptInputs), dismissed: true })).toBe("hidden");
  });
});

describe("PushPrompt", () => {
  it("offers push on a browser that can say yes and has not been asked", async () => {
    fakePushBrowser();
    renderPrompts();
    expect(await screen.findByTestId("push-prompt")).toHaveTextContent(
      "Get notifications on this device"
    );
    expect(screen.getByTestId("push-prompt-turn-on")).toBeEnabled();
    expect(screen.getByTestId("push-prompt-dismiss")).toBeInTheDocument();
  });

  it("shows nothing where the deployment has no push key", async () => {
    const browser = fakePushBrowser();
    service.vapidPublicKey.mockResolvedValue({ configured: false, public_key: null });
    renderPrompts();
    await settled();
    expect(screen.queryByTestId("push-prompt")).toBeNull();
    expect(browser.serviceWorker.getRegistration).not.toHaveBeenCalled();
  });

  it("shows nothing in a browser without push", async () => {
    renderPrompts();
    await settled();
    expect(screen.queryByTestId("push-prompt")).toBeNull();
  });

  it("shows nothing once notifications are blocked, or already allowed", async () => {
    for (const permission of ["denied", "granted"] as const) {
      fakePushBrowser({ permission });
      renderPrompts();
      await settled();
      expect(screen.queryByTestId("push-prompt"), permission).toBeNull();
      cleanup();
    }
  });

  it("shows nothing on a device that is already registered", async () => {
    const browser = fakePushBrowser({ subscribed: true });
    renderPrompts();
    await waitFor(() => expect(browser.serviceWorker.getRegistration).toHaveBeenCalled());
    await settled();
    expect(screen.queryByTestId("push-prompt")).toBeNull();
  });

  it("hides for 30 days once dismissed, and comes back after", async () => {
    const browser = fakePushBrowser();
    renderPrompts();
    fireEvent.click(await screen.findByTestId("push-prompt-dismiss"));
    expect(screen.queryByTestId("push-prompt")).toBeNull();
    expect(window.localStorage.getItem(DISMISSED_AT_KEY)).toBe(String(NOW));
    expect(browser.notification.requestPermission).not.toHaveBeenCalled();

    // A day short of 30 later, a fresh page still keeps it hidden.
    cleanup();
    vi.setSystemTime(NOW + DISMISS_FOR_MS - 24 * 60 * 60 * 1000);
    renderPrompts();
    await settled();
    expect(screen.queryByTestId("push-prompt")).toBeNull();

    cleanup();
    vi.setSystemTime(NOW + DISMISS_FOR_MS);
    renderPrompts();
    expect(await screen.findByTestId("push-prompt")).toBeInTheDocument();
  });

  it("still hides when this browser will not store the dismissal", async () => {
    fakePushBrowser();
    const blocked = () => {
      throw new DOMException("blocked", "SecurityError");
    };
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(blocked);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(blocked);
    vi.spyOn(console, "warn").mockImplementation(() => {});
    renderPrompts();
    fireEvent.click(await screen.findByTestId("push-prompt-dismiss"));
    expect(screen.queryByTestId("push-prompt")).toBeNull();
    vi.restoreAllMocks();
  });

  it("hides every prompt on the page when one is dismissed", async () => {
    fakePushBrowser();
    renderPrompts(2);
    await waitFor(() => expect(screen.getAllByTestId("push-prompt")).toHaveLength(2));
    fireEvent.click(screen.getAllByTestId("push-prompt-dismiss")[0]);
    expect(screen.queryAllByTestId("push-prompt")).toHaveLength(0);
  });

  it("hides every prompt on the page once push is turned on in one", async () => {
    fakePushBrowser();
    renderPrompts(2);
    await waitFor(() => expect(screen.getAllByTestId("push-prompt")).toHaveLength(2));
    fireEvent.click(screen.getAllByTestId("push-prompt-turn-on")[0]);
    await waitFor(() => expect(service.registerDevice).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryAllByTestId("push-prompt")).toHaveLength(0));
  });

  it("asks for permission inside the click, never before it", async () => {
    const browser = fakePushBrowser();
    renderPrompts();
    const turnOn = await screen.findByTestId("push-prompt-turn-on");
    expect(browser.notification.requestPermission).not.toHaveBeenCalled();

    fireEvent.click(turnOn);
    // Synchronously, in the click's own task: a request made after an `await`
    // can fall outside the user gesture, and Safari then refuses it unseen.
    expect(browser.notification.requestPermission).toHaveBeenCalledTimes(1);

    await waitFor(() =>
      expect(service.registerDevice).toHaveBeenCalledWith({
        platform: "web",
        token: FAKE_SUBSCRIPTION.endpoint,
        device_name: expect.any(String),
        keys: FAKE_SUBSCRIPTION.keys
      })
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Notifications are on for this device")
    );
    // The same flow as Settings: the id is remembered, so Devices says "This device".
    expect(thisDeviceId()).toBe("d-9");
    expect(screen.queryByTestId("push-prompt")).toBeNull();
  });

  it("registers nothing when the browser says no", async () => {
    fakePushBrowser({ answer: "denied" });
    renderPrompts();
    fireEvent.click(await screen.findByTestId("push-prompt-turn-on"));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(service.registerDevice).not.toHaveBeenCalled();
    // Blocked now, so the row goes: Settings explains a block, the inbox does not.
    await waitFor(() => expect(screen.queryByTestId("push-prompt")).toBeNull());
  });

  it("on an iPhone in a browser tab, says to add Oxygen to the Home Screen", async () => {
    device.homeScreen = true;
    renderPrompts();
    expect(await screen.findByTestId("push-prompt-home-screen")).toHaveTextContent(
      HOME_SCREEN_HINT
    );
    expect(screen.queryByTestId("push-prompt-turn-on")).toBeNull();
    fireEvent.click(screen.getByTestId("push-prompt-dismiss"));
    expect(screen.queryByTestId("push-prompt")).toBeNull();
  });
});
