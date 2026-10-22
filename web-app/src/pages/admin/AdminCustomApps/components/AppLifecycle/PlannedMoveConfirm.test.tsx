// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { MovePlan } from "@/types/appPromote";
import { PlannedMoveConfirm } from "./PlannedMoveConfirm";

vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { moveProduction: vi.fn() } }));

const plan = (over: Partial<MovePlan> = {}): MovePlan => ({
  action: "rollback",
  app_id: "app-1",
  from: null,
  to: "production",
  build: { id: "old", build_id: "cccccccccccccccc", published_by: null, created_at: "" },
  current: { id: "live", build_id: "aaaaaaaaaaaaaaaa" },
  expect_current: "live",
  served_since: null,
  functions: { added: [], removed: [], changed: [] },
  schedules: { added: [], removed: [], changed: [] },
  migrations: { when: "before_move", oltp: [], airhouse: [] },
  required_secrets_unset: [],
  blockers: [],
  warnings: [],
  dry_run: true,
  moved: false,
  ...over
});

const nightly = { function: "nightly", cron: "0 0 * * *", timezone: "UTC" };
const COPY = { title: "Roll back?", description: "Customers will see …", confirm: "Roll back" };

const renderConfirm = (onClose = vi.fn()) => {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PlannedMoveConfirm
        appId='app-1'
        target={{ act: "rollback", buildId: "old" }}
        copy={COPY}
        onClose={onClose}
        testId='move'
      />
    </QueryClientProvider>
  );
  return onClose;
};

const acts = () =>
  vi.mocked(CustomAppsService.moveProduction).mock.calls.filter(([, , body]) => !body.dry_run);
const dryRuns = () =>
  vi.mocked(CustomAppsService.moveProduction).mock.calls.filter(([, , body]) => body.dry_run);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PlannedMoveConfirm", () => {
  it("does not hold on a schedule removal, and confirms it by acting", async () => {
    const removal = {
      code: "schedule_removal_unconfirmed",
      message: "retires nightly"
    };
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) =>
      body.dry_run
        ? plan({ blockers: [removal], schedules: { added: [], removed: [nightly], changed: [] } })
        : plan({ dry_run: false, moved: true })
    );
    const onClose = renderConfirm();
    await screen.findByTestId("app-lifecycle-plan");
    await vi.waitFor(() => expect(screen.getByTestId("move-ok")).toBeEnabled());
    await userEvent.click(screen.getByTestId("move-ok"));
    await vi.waitFor(() => expect(acts()).toHaveLength(1));
    expect(acts()[0][2]).toEqual({
      expect_current: "live",
      confirm_schedule_removals: ["nightly"]
    });
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("holds on any other blocker", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue(
      plan({ blockers: [{ code: "staging_never_served", message: "never served" }] })
    );
    renderConfirm();
    await screen.findByTestId("app-lifecycle-plan-blocker-staging_never_served");
    expect(screen.getByTestId("move-ok")).toBeDisabled();
  });

  it("re-reads the plan after a refused act and stays open", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) => {
      if (body.dry_run) return plan();
      throw new Error("target_moved");
    });
    const onClose = renderConfirm();
    await screen.findByTestId("app-lifecycle-plan");
    await vi.waitFor(() => expect(screen.getByTestId("move-ok")).toBeEnabled());
    await userEvent.click(screen.getByTestId("move-ok"));
    await vi.waitFor(() => expect(dryRuns()).toHaveLength(2));
    expect(screen.getByTestId("move")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
