// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationSettings } from "@/types/notifications";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: {
    settings: vi.fn(),
    updateSettings: vi.fn(),
    catalog: vi.fn(),
    devices: vi.fn(),
    updateDevice: vi.fn(),
    removeDevice: vi.fn(),
    vapidPublicKey: vi.fn(),
    registerDevice: vi.fn()
  }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("@/libs/webPush", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/libs/webPush")>()),
  pushSupported: () => false,
  pushNeedsHomeScreenHere: () => false
}));

import { applySettingsPatch } from "@/hooks/api/notifications/settingsPatch";
import AccountNotifications from ".";
import { oxygen, settingsDoc, storeOps } from "./__tests__/fixtures";

const service = vi.mocked(NotificationsService);

beforeAll(() => {
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});

/** Serve `initial`, and keep every write the way the server would, bumping the version. */
function fakeServer(initial: NotificationSettings = settingsDoc()) {
  let current = initial;
  let n = 1;
  service.settings.mockImplementation(async () => current);
  service.updateSettings.mockImplementation(async (patch) => {
    n += 1;
    current = { ...applySettingsPatch(current, patch), version: `v${n}` };
    return current;
  });
}

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

/** `source`, `cat-<path>` or `kind-<kind>`, then the channel or `menu`. */
const id = (row: string, part: string, testKey = "store-ops") =>
  `notifications-pref-${testKey}-${row}-${part}`;
const control = (row: string, part: string, testKey?: string) =>
  screen.findByTestId(id(row, part, testKey));
const note = (row: string, testKey?: string) => screen.getByTestId(id(row, "note", testKey));
const sentNodes = () => service.updateSettings.mock.calls.map(([patch]) => patch.nodes);

beforeEach(() => {
  fakeServer();
  service.catalog.mockResolvedValue({ sources: [], reach: { push_devices: 0, email: true } });
  service.devices.mockResolvedValue([]);
  service.vapidPublicKey.mockResolvedValue({ configured: true, public_key: "BAAA" });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Account → Notifications tree", () => {
  it("draws Oxygen first, then each app as its tree", async () => {
    renderSection();
    await screen.findByTestId("notifications-group-store-ops");
    const groups = screen.getAllByTestId(/^notifications-group-/);
    expect(groups.map((g) => g.getAttribute("data-testid"))).toEqual([
      "notifications-group-oxygen",
      "notifications-group-store-ops"
    ]);
    const rows = within(groups[1]).getAllByTestId(/-row$/);
    expect(rows.map((r) => r.getAttribute("data-testid"))).toEqual([
      id("source", "row"),
      id("cat-operations", "row"),
      id("kind-list_overdue", "row"),
      id("cat-operations/food-safety", "row"),
      id("kind-temp_check", "row"),
      id("kind-temp_alarm", "row"),
      id("cat-staffing", "row"),
      id("kind-shift_swap", "row"),
      id("kind-swap_urgent", "row")
    ]);
  });

  it("shows an inherited switch as what it gets, from where, and still lets it be set here", async () => {
    renderSection();
    const email = await control("kind-list_overdue", "email");
    expect(email).not.toBeChecked();
    expect(email).toBeEnabled();
    expect(email).toHaveAccessibleName("Store Ops, Overdue lists: Email, from Store operations");
    expect(note("kind-list_overdue")).toHaveTextContent("Email from Store operations");
    expect(screen.queryByTestId(id("kind-list_overdue", "email-reset"))).toBeNull();
  });

  it("turns a channel back on under a row that turned it off", async () => {
    renderSection();
    await userEvent.click(await control("kind-list_overdue", "email"));
    await waitFor(() =>
      expect(sentNodes()).toEqual([
        [{ app_id: "app-1", kind: "list_overdue", channels: { email: true } }]
      ])
    );
    expect(await control("kind-list_overdue", "email")).toBeChecked();
    expect(await control("kind-list_overdue", "email-reset")).toBeInTheDocument();
  });

  it("resets a row's own setting back to inheriting", async () => {
    renderSection();
    expect(await control("cat-operations", "email")).not.toBeChecked();
    await userEvent.click(await control("cat-operations", "email-reset"));
    await waitFor(() =>
      expect(sentNodes()).toEqual([
        [{ app_id: "app-1", scope: "operations", channels: { email: null } }]
      ])
    );
    // Back to the default, and nothing under it says it comes from Store operations.
    expect(await control("cat-operations", "email")).toBeChecked();
    expect(screen.queryByTestId(id("cat-operations", "email-reset"))).toBeNull();
    expect(screen.queryByTestId(id("kind-list_overdue", "note"))).toBeNull();
  });

  it("writes the whole source with scope '' and Oxygen with app_id null", async () => {
    renderSection();
    await userEvent.click(await control("source", "push"));
    await userEvent.click(await control("cat-runs", "email", "oxygen"));
    await waitFor(() =>
      expect(sentNodes()).toEqual([
        [{ app_id: "app-1", scope: "", channels: { push: false } }],
        [{ app_id: null, scope: "runs", channels: { email: false } }]
      ])
    );
  });

  it("locks a critical kind, and a category of nothing else, on — no reset, no floor", async () => {
    renderSection();
    for (const channel of ["push", "email", "slack"]) {
      const toggle = await control("kind-temp_alarm", channel);
      expect(toggle).toBeChecked();
      expect(toggle).toBeDisabled();
    }
    expect(note("kind-temp_alarm")).toHaveTextContent("Critical — always delivered");
    expect(screen.queryByTestId(id("kind-temp_alarm", "menu"))).toBeNull();
    expect(await control("cat-billing", "push", "oxygen")).toBeDisabled();
    expect(screen.queryByTestId(id("cat-billing", "menu", "oxygen"))).toBeNull();
  });

  it("shows a kind under a floor above its level as inbox only, its switches off", async () => {
    renderSection();
    const push = await control("kind-shift_swap", "push");
    expect(push).not.toBeChecked();
    expect(push).toBeDisabled();
    expect(note("kind-shift_swap")).toHaveTextContent("Inbox only — floor on Staffing");
    expect(await control("kind-swap_urgent", "push")).toBeChecked();
  });

  it("draws a dash, not a switch, for a channel nothing under the row uses", async () => {
    renderSection();
    expect(await control("cat-runs", "slack-none", "oxygen")).toBeInTheDocument();
    expect(screen.queryByTestId(id("cat-runs", "slack", "oxygen"))).toBeNull();
  });

  it("disables email for an account with no address", async () => {
    fakeServer(settingsDoc({ reach: { push_devices: 0, email: false, slack: true } }));
    renderSection();
    expect(await control("source", "email")).toBeDisabled();
    expect(await control("source", "push")).toBeEnabled();
    expect(screen.getByTestId("notifications-no-email")).toBeInTheDocument();
  });

  it("puts a switch back and says so when the server refuses", async () => {
    renderSection();
    service.updateSettings.mockRejectedValueOnce(new Error("boom"));
    await userEvent.click(await control("kind-temp_check", "push"));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't save that setting"));
    expect(await control("kind-temp_check", "push")).toBeChecked();
    expect(screen.queryByTestId(id("kind-temp_check", "push-reset"))).toBeNull();
  });

  it("says so when nothing has sent anything yet", async () => {
    fakeServer(settingsDoc({ sources: [] }));
    renderSection();
    expect(await screen.findByTestId("notifications-prefs-empty")).toBeInTheDocument();
  });

  it("says so when the settings cannot be read", async () => {
    service.settings.mockRejectedValue(new Error("down"));
    renderSection();
    expect(await screen.findByText("Couldn't load your notification settings")).toBeInTheDocument();
  });
});

describe("Account → Notifications level floor", () => {
  it("keeps the floor behind '…', showing the row's current one", async () => {
    renderSection();
    expect(screen.queryByTestId(id("cat-staffing", "floor"))).toBeNull();
    await userEvent.click(await control("cat-staffing", "menu"));
    const menu = await screen.findByTestId(id("cat-staffing", "floor"));
    expect(within(menu).getByTestId(id("cat-staffing", "floor-inherit"))).toHaveTextContent(
      "Same as Store Ops"
    );
    expect(within(menu).getByTestId(id("cat-staffing", "floor-time_sensitive"))).toHaveAttribute(
      "aria-checked",
      "true"
    );
  });

  it("sets a row's floor", async () => {
    renderSection();
    await userEvent.click(await control("kind-temp_check", "menu"));
    await userEvent.click(await screen.findByTestId(id("kind-temp_check", "floor-critical")));
    await waitFor(() =>
      expect(sentNodes()).toEqual([
        [{ app_id: "app-1", kind: "temp_check", min_level: "critical" }]
      ])
    );
    expect(note("kind-temp_check")).toHaveTextContent("Inbox only");
  });

  it("goes back to the row above's floor with 'Same as'", async () => {
    renderSection();
    await userEvent.click(await control("cat-staffing", "menu"));
    await userEvent.click(await screen.findByTestId(id("cat-staffing", "floor-inherit")));
    await waitFor(() =>
      expect(sentNodes()).toEqual([[{ app_id: "app-1", scope: "staffing", min_level: null }]])
    );
    expect(await control("kind-shift_swap", "push")).toBeEnabled();
  });

  it("on the source, 'Everything' is no floor at all", async () => {
    fakeServer(
      settingsDoc({
        sources: [{ ...storeOps(), settings: { channels: {}, min_level: "critical" } }, oxygen()]
      })
    );
    renderSection();
    await userEvent.click(await control("source", "menu"));
    expect(screen.queryByTestId(id("source", "floor-inherit"))).toBeNull();
    await userEvent.click(await screen.findByTestId(id("source", "floor-passive")));
    await waitFor(() =>
      expect(sentNodes()).toEqual([[{ app_id: "app-1", scope: "", min_level: null }]])
    );
  });
});

describe("Account → Notifications pause and quiet hours", () => {
  it("pauses from a preset and resumes", async () => {
    renderSection();
    await userEvent.click(await screen.findByTestId("notifications-pause-open"));
    await userEvent.click(await screen.findByTestId("notifications-pause-hour"));
    await waitFor(() => expect(service.updateSettings).toHaveBeenCalledTimes(1));
    const until = service.updateSettings.mock.calls[0][0].global?.paused_until;
    expect(new Date(until ?? 0).getTime() - Date.now()).toBeGreaterThan(59 * 60 * 1000);
    expect(await screen.findByTestId("notifications-pause-status")).toHaveTextContent(
      /^Paused until /
    );

    await userEvent.click(await screen.findByTestId("notifications-pause-resume"));
    await waitFor(() =>
      expect(service.updateSettings).toHaveBeenLastCalledWith(
        { global: { paused_until: null } },
        "v2"
      )
    );
    expect(await screen.findByTestId("notifications-pause-status")).toHaveTextContent(
      "Pause notifications"
    );
  });

  it("turns quiet hours on with this browser's time zone the first time", async () => {
    renderSection();
    await userEvent.click(await screen.findByTestId("notifications-quiet-toggle"));
    await waitFor(() => expect(service.updateSettings).toHaveBeenCalledTimes(1));
    expect(service.updateSettings.mock.calls[0][0]).toEqual({
      global: {
        quiet_hours: {
          start: "22:00",
          end: "07:00",
          days: [0, 1, 2, 3, 4, 5, 6],
          allow: "critical"
        },
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone
      }
    });
    expect(await screen.findByTestId("notifications-quiet-start")).toHaveValue("22:00");
  });
});
