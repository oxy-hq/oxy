// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type {
  KindNode,
  NotificationCatalog,
  NotificationReach,
  SourceNode
} from "@/types/notifications";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    catalog: vi.fn(),
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

import { withPreference } from "@/hooks/api/notifications";
import AccountNotifications from ".";

const service = vi.mocked(NotificationsService);

const kind = (over: Partial<KindNode> & { kind: string; label: string }): KindNode => ({
  description: null,
  level: "active",
  critical: false,
  channels: ["push", "email", "slack"],
  muted: [],
  actions: [],
  unread: 0,
  ...over
});

const oxygen: SourceNode = {
  source: "oxygen",
  app: null,
  label: "Oxygen",
  muted: [],
  unread: 0,
  categories: [
    {
      path: "billing",
      label: "Billing",
      description: null,
      muted: [],
      unread: 0,
      categories: [],
      kinds: [
        kind({
          kind: "oxy.billing_past_due",
          label: "Payment past due",
          level: "critical",
          critical: true,
          channels: ["push"]
        })
      ]
    },
    {
      path: "runs",
      label: "Automations and pipelines",
      description: null,
      muted: [],
      unread: 0,
      categories: [],
      kinds: [kind({ kind: "oxy.run_failed", label: "Run failed", channels: ["push", "email"] })]
    }
  ]
};

const storeOps = (over: Partial<SourceNode> = {}): SourceNode => ({
  source: "app",
  app: { id: "app-1", slug: "store-ops", name: "Store Ops", org_slug: "acme" },
  label: "Store Ops",
  muted: [],
  unread: 0,
  categories: [
    {
      path: "operations",
      label: "Store operations",
      description: null,
      muted: ["email"],
      unread: 0,
      kinds: [kind({ kind: "list_overdue", label: "Overdue lists", level: "time_sensitive" })],
      categories: [
        {
          path: "operations/food-safety",
          label: "Food safety",
          description: "Temperature logs and checks",
          muted: [],
          unread: 0,
          categories: [],
          kinds: [
            kind({ kind: "temp_check", label: "Temperature check" }),
            kind({
              kind: "temp_alarm",
              label: "Temperature alarm",
              level: "critical",
              critical: true,
              muted: ["push"]
            })
          ]
        }
      ]
    }
  ],
  ...over
});

const catalog = (
  sources: SourceNode[] = [storeOps()],
  reach: Partial<NotificationReach> = {}
): NotificationCatalog => ({
  sources,
  reach: { push_devices: 2, email: true, ...reach }
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

/** A switch: `source`, `cat-<path>` or `kind-<kind>`, then the channel. */
const pref = (row: string, channel: string, testKey = "store-ops") =>
  screen.findByTestId(`notifications-pref-${testKey}-${row}-${channel}`);
const note = (row: string, testKey = "store-ops") =>
  screen.getByTestId(`notifications-pref-${testKey}-${row}-note`);

/** Serve `initial`, and keep every write: what the next read returns, as the server would. */
function fakeServer(initial: NotificationCatalog) {
  let current = initial;
  service.catalog.mockImplementation(async () => current);
  service.setPreference.mockImplementation(async (input) => {
    current = withPreference(current, input);
  });
}

beforeEach(() => {
  browser.supported = false;
  browser.homeScreen = false;
  service.catalog.mockResolvedValue(catalog());
  service.setPreference.mockResolvedValue();
  service.vapidPublicKey.mockResolvedValue({ configured: true, public_key: "BAAA" });
  service.registerDevice.mockResolvedValue();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Account → Notifications tree", () => {
  it("draws each source as its tree: the source, its categories, their kinds", async () => {
    renderSection();
    const rows = await screen.findAllByTestId(/^notifications-pref-store-ops-.*-row$/);
    expect(rows.map((r) => r.getAttribute("data-testid"))).toEqual([
      "notifications-pref-store-ops-source-row",
      "notifications-pref-store-ops-cat-operations-row",
      "notifications-pref-store-ops-kind-list_overdue-row",
      "notifications-pref-store-ops-cat-operations/food-safety-row",
      "notifications-pref-store-ops-kind-temp_check-row",
      "notifications-pref-store-ops-kind-temp_alarm-row"
    ]);
    expect(rows[3]).toHaveTextContent("Food safety");
    expect(rows[3]).toHaveTextContent("Temperature logs and checks");
  });

  it("shows a mute set at a node as that node's switch off, the rest on", async () => {
    renderSection();
    expect(await pref("cat-operations", "email")).not.toBeChecked();
    expect(await pref("cat-operations", "email")).toBeEnabled();
    expect(await pref("cat-operations", "push")).toBeChecked();
    expect(await pref("source", "email")).toBeChecked();
  });

  it("shows every child of a muted node off and locked, muted by that node", async () => {
    renderSection();
    for (const row of ["kind-list_overdue", "cat-operations/food-safety", "kind-temp_check"]) {
      const toggle = await pref(row, "email");
      expect(toggle).not.toBeChecked();
      expect(toggle).toBeDisabled();
      expect(toggle).toHaveAccessibleName(/muted by Store operations$/);
      expect(note(row)).toHaveTextContent("Muted by Store operations");
    }
    // Only the muted channel is inherited.
    expect(await pref("kind-list_overdue", "push")).toBeEnabled();
    expect(await pref("kind-list_overdue", "push")).toBeChecked();
  });

  it("names the source when the whole source is muted", async () => {
    service.catalog.mockResolvedValue(catalog([storeOps({ muted: ["push"] })]));
    renderSection();
    const toggle = await pref("cat-operations/food-safety", "push");
    expect(toggle).toBeDisabled();
    expect(note("cat-operations/food-safety")).toHaveTextContent(
      "Muted by Store Ops, Store operations"
    );
  });

  it("locks a critical kind on, whatever is muted above or at it, and says why", async () => {
    service.catalog.mockResolvedValue(catalog([storeOps({ muted: ["push", "slack"] })]));
    renderSection();
    for (const channel of ["push", "email", "slack"]) {
      const toggle = await pref("kind-temp_alarm", channel);
      expect(toggle).toBeDisabled();
      expect(toggle).toBeChecked();
    }
    expect(note("kind-temp_alarm")).toHaveTextContent("Critical — always delivered");
    await userEvent.click(await pref("kind-temp_alarm", "slack"));
    expect(service.setPreference).not.toHaveBeenCalled();
  });

  it("disables email for an account with no address", async () => {
    service.catalog.mockResolvedValue(catalog([storeOps()], { email: false }));
    renderSection();
    expect(await screen.findByTestId("notifications-no-email")).toHaveTextContent(
      "No email on this account"
    );
    expect(await pref("source", "email")).toBeDisabled();
    expect(await pref("source", "push")).toBeEnabled();
  });

  it("says so when nothing has sent anything yet", async () => {
    service.catalog.mockResolvedValue(catalog([]));
    renderSection();
    expect(await screen.findByTestId("notifications-prefs-empty")).toBeInTheDocument();
  });
});

describe("Account → Notifications writes", () => {
  it("mutes a whole source with scope ''", async () => {
    fakeServer(catalog());
    renderSection();
    await userEvent.click(await pref("source", "push"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: "app-1",
        scope: "",
        channel: "push",
        muted: true
      })
    );
    // The write covers the whole subtree.
    await waitFor(async () => expect(await pref("kind-list_overdue", "push")).toBeDisabled());
  });

  it("unmutes a category by its path", async () => {
    fakeServer(catalog());
    renderSection();
    await userEvent.click(await pref("cat-operations", "email"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: "app-1",
        scope: "operations",
        channel: "email",
        muted: false
      })
    );
    await waitFor(async () => expect(await pref("kind-list_overdue", "email")).toBeEnabled());
  });

  it("mutes one kind with the kind form", async () => {
    renderSection();
    await userEvent.click(await pref("kind-list_overdue", "slack"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: "app-1",
        kind: "list_overdue",
        channel: "slack",
        muted: true
      })
    );
  });
});

describe("Account → Notifications Oxygen", () => {
  it("shows Oxygen above the apps, whatever order the server sends", async () => {
    service.catalog.mockResolvedValue(catalog([storeOps(), oxygen]));
    renderSection();
    await screen.findByTestId("notifications-group-oxygen");
    const order = screen
      .getAllByTestId(/^notifications-group-/)
      .map((el) => el.getAttribute("data-testid"));
    expect(order).toEqual(["notifications-group-oxygen", "notifications-group-store-ops"]);
  });

  it("mutes an Oxygen category with app_id null", async () => {
    service.catalog.mockResolvedValue(catalog([oxygen]));
    renderSection();
    await userEvent.click(await pref("cat-runs", "email", "oxygen"));
    await waitFor(() =>
      expect(service.setPreference).toHaveBeenCalledWith({
        app_id: null,
        scope: "runs",
        channel: "email",
        muted: true
      })
    );
  });

  it("locks billing — and its category, which holds nothing else — on", async () => {
    service.catalog.mockResolvedValue(catalog([oxygen]));
    renderSection();
    for (const row of ["kind-oxy.billing_past_due", "cat-billing"]) {
      const toggle = await pref(row, "push", "oxygen");
      expect(toggle).toBeDisabled();
      expect(toggle).toBeChecked();
    }
    // The source holds a kind that is not critical, so it can be muted.
    expect(await pref("source", "push", "oxygen")).toBeEnabled();
  });

  it("offers a switch only for the channels something under the row is sent on", async () => {
    service.catalog.mockResolvedValue(catalog([oxygen]));
    renderSection();
    await screen.findByTestId("notifications-group-oxygen");
    const id = (row: string, channel: string) => `notifications-pref-oxygen-${row}-${channel}`;
    expect(screen.getByTestId(id("kind-oxy.run_failed", "email"))).toBeInTheDocument();
    expect(screen.queryByTestId(id("kind-oxy.run_failed", "slack"))).toBeNull();
    expect(screen.getByTestId(id("kind-oxy.run_failed", "slack-none"))).toBeInTheDocument();
    expect(screen.queryByTestId(id("cat-billing", "email"))).toBeNull();
    // The source covers push and email between its kinds, and no Slack.
    expect(screen.getByTestId(id("source", "email"))).toBeInTheDocument();
    expect(screen.getByTestId(id("source", "slack-none"))).toBeInTheDocument();
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
