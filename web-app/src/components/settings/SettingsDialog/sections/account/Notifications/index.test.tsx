// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationPreferences } from "@/types/notifications";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    preferences: vi.fn(),
    setPreference: vi.fn(),
    vapidPublicKey: vi.fn(),
    registerDevice: vi.fn()
  }
}));

const browser = vi.hoisted(() => ({ supported: false, homeScreen: false }));
vi.mock("@/libs/webPush", () => ({
  pushSupported: () => browser.supported,
  pushNeedsHomeScreenHere: () => browser.homeScreen,
  subscribeThisDevice: vi.fn().mockResolvedValue("https://push.example/sub/abc")
}));

import AccountNotifications from ".";

const service = vi.mocked(NotificationsService);

const platform: NotificationPreferences["platform"] = [
  { kind: "oxy.billing_past_due", muted: [], critical: true, channels: ["push"] },
  { kind: "oxy.run_failed", muted: ["email"], critical: false, channels: ["push", "email"] }
];

const prefs = (over: Partial<NotificationPreferences["reach"]> = {}): NotificationPreferences => ({
  platform: [],
  apps: [
    {
      app: { id: "app-1", slug: "store-ops", name: "Store Ops" },
      kinds: [
        { kind: "list_overdue", muted: ["email"], critical: false },
        { kind: "temp_alarm", muted: ["push"], critical: true }
      ]
    }
  ],
  reach: { push_devices: 2, email: true, ...over }
});

const renderSection = () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
  render(
    <QueryClientProvider client={queryClient}>
      <AccountNotifications />
    </QueryClientProvider>
  );
};

const pref = (kind: string, channel: string) =>
  screen.findByTestId(`notifications-pref-store-ops-${kind}-${channel}`);

beforeEach(() => {
  browser.supported = false;
  browser.homeScreen = false;
  service.preferences.mockResolvedValue(prefs());
  service.setPreference.mockResolvedValue();
  service.vapidPublicKey.mockResolvedValue({ configured: true, public_key: "BAAA" });
  service.registerDevice.mockResolvedValue();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Account → Notifications switches", () => {
  it("names each kind for reading and shows on for every channel that is not muted", async () => {
    renderSection();
    expect(await screen.findByText("List overdue")).toBeInTheDocument();
    expect(await pref("list_overdue", "push")).toBeChecked();
    expect(await pref("list_overdue", "email")).not.toBeChecked();
    expect(await pref("list_overdue", "slack")).toBeChecked();
  });

  it("sends muted = !checked for the switch that moved", async () => {
    renderSection();
    await userEvent.click(await pref("list_overdue", "push"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: "app-1",
        kind: "list_overdue",
        channel: "push",
        muted: true
      })
    );

    await userEvent.click(await pref("list_overdue", "email"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenLastCalledWith({
        app_id: "app-1",
        kind: "list_overdue",
        channel: "email",
        muted: false
      })
    );
  });

  it("locks a critical kind on, whatever was muted, and says why", async () => {
    renderSection();
    for (const channel of ["push", "email", "slack"]) {
      const toggle = await pref("temp_alarm", channel);
      expect(toggle).toBeDisabled();
      expect(toggle).toBeChecked();
    }
    expect(screen.getByTestId("notifications-critical-store-ops-temp_alarm")).toHaveTextContent(
      "Critical — always delivered"
    );
    await userEvent.click(await pref("temp_alarm", "slack"));
    expect(service.setPreference).not.toHaveBeenCalled();
  });

  it("disables email for an account with no address", async () => {
    service.preferences.mockResolvedValue(prefs({ email: false }));
    renderSection();
    expect(await screen.findByTestId("notifications-no-email")).toHaveTextContent(
      "No email on this account"
    );
    expect(await pref("list_overdue", "email")).toBeDisabled();
    expect(await pref("list_overdue", "push")).toBeEnabled();
  });

  it("says so when nothing has sent anything yet", async () => {
    service.preferences.mockResolvedValue({
      apps: [],
      platform: [],
      reach: { push_devices: 0, email: true }
    });
    renderSection();
    expect(await screen.findByTestId("notifications-prefs-empty")).toBeInTheDocument();
  });
});

describe("Account → Notifications Oxygen's own kinds", () => {
  it("shows them as an Oxygen group above the apps, named for reading", async () => {
    service.preferences.mockResolvedValue({ ...prefs(), platform });
    renderSection();
    const group = await screen.findByTestId("notifications-group-oxygen");
    expect(group).toHaveTextContent("Oxygen");
    expect(group).toHaveTextContent("Billing past due");
    expect(group).toHaveTextContent("Run failed");
    const order = screen
      .getAllByTestId(/^notifications-group-/)
      .map((el) => el.getAttribute("data-testid"));
    expect(order).toEqual(["notifications-group-oxygen", "notifications-group-store-ops"]);
  });

  it("mutes an Oxygen kind with app_id null", async () => {
    service.preferences.mockResolvedValue({ ...prefs(), platform });
    renderSection();
    const toggle = await screen.findByTestId("notifications-pref-oxygen-oxy.run_failed-push");
    expect(toggle).toBeChecked();
    await userEvent.click(toggle);
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: null,
        kind: "oxy.run_failed",
        channel: "push",
        muted: true
      })
    );
  });

  it("locks billing past due on", async () => {
    service.preferences.mockResolvedValue({ ...prefs(), platform });
    renderSection();
    const toggle = await screen.findByTestId("notifications-pref-oxygen-oxy.billing_past_due-push");
    expect(toggle).toBeDisabled();
    expect(toggle).toBeChecked();
    expect(
      screen.getByTestId("notifications-critical-oxygen-oxy.billing_past_due")
    ).toBeInTheDocument();
  });

  it("offers a switch only for the channels a kind is sent on", async () => {
    service.preferences.mockResolvedValue({ ...prefs(), platform });
    renderSection();
    await screen.findByTestId("notifications-group-oxygen");
    expect(
      screen.getByTestId("notifications-pref-oxygen-oxy.run_failed-email")
    ).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-pref-oxygen-oxy.run_failed-slack")).toBeNull();
    expect(
      screen.getByTestId("notifications-pref-oxygen-oxy.run_failed-slack-none")
    ).toBeInTheDocument();
    for (const channel of ["email", "slack"]) {
      expect(
        screen.queryByTestId(`notifications-pref-oxygen-oxy.billing_past_due-${channel}`)
      ).toBeNull();
    }
    // An app's kinds, and a server too old to say, keep every switch.
    expect(
      screen.getByTestId("notifications-pref-store-ops-list_overdue-slack")
    ).toBeInTheDocument();
  });

  it("has no Oxygen group when Oxygen has sent nothing", async () => {
    renderSection();
    await screen.findByTestId("notifications-group-store-ops");
    expect(screen.queryByTestId("notifications-group-oxygen")).toBeNull();
  });

  it("shows the Oxygen group alone when no app has sent anything", async () => {
    service.preferences.mockResolvedValue({ ...prefs(), apps: [], platform });
    renderSection();
    expect(await screen.findByTestId("notifications-group-oxygen")).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-prefs-empty")).toBeNull();
  });
});

describe("Account → Notifications push on this device", () => {
  it("is not available where the deployment has no push key", async () => {
    browser.supported = true;
    service.vapidPublicKey.mockResolvedValue({ configured: false, public_key: null });
    renderSection();
    expect(await screen.findByTestId("notifications-push-unavailable")).toHaveTextContent(
      "Not available here"
    );
    expect(screen.queryByTestId("notifications-push-enable")).toBeNull();
  });

  it("is not available in a browser without push", async () => {
    renderSection();
    expect(await screen.findByTestId("notifications-push-unavailable")).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-push-home-screen")).toBeNull();
  });

  it("on an iPhone in a browser tab, says to add Oxygen to the Home Screen", async () => {
    browser.homeScreen = true;
    renderSection();
    expect(await screen.findByTestId("notifications-push-home-screen")).toHaveTextContent(
      "Add Oxygen to your Home Screen (Share → Add to Home Screen) to get notifications here."
    );
    expect(screen.queryByTestId("notifications-push-unavailable")).toBeNull();
    expect(screen.queryByTestId("notifications-push-enable")).toBeNull();
  });

  it("has no Home Screen hint where the deployment has no push key", async () => {
    browser.homeScreen = true;
    service.vapidPublicKey.mockResolvedValue({ configured: false, public_key: null });
    renderSection();
    expect(await screen.findByTestId("notifications-push-unavailable")).toBeInTheDocument();
    expect(screen.queryByTestId("notifications-push-home-screen")).toBeNull();
  });

  it("shows how many devices push reaches", async () => {
    renderSection();
    expect(await screen.findByTestId("notifications-push-devices")).toHaveTextContent(
      "Reaches 2 devices"
    );
  });

  it("subscribes this browser and registers its endpoint", async () => {
    browser.supported = true;
    renderSection();
    await userEvent.click(await screen.findByTestId("notifications-push-enable"));
    await waitFor(() =>
      expect(service.registerDevice).toHaveBeenCalledWith({
        platform: "web",
        token: "https://push.example/sub/abc"
      })
    );
    expect(await screen.findByTestId("notifications-push-enable")).toHaveTextContent("On");
  });
});
