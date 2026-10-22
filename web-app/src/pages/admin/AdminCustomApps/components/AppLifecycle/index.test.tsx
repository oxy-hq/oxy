// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppLifecycle as Lifecycle } from "@/hooks/api/customApps/deriveLifecycle";
import { CustomAppsService } from "@/services/api/customApps";
import type { MovePlan } from "@/types/appPromote";
import type { AppBuild, CustomApp } from "@/types/apps";
import { lifecycleVerdict } from "../../appStatus";

vi.mock("@/services/api/customApps", () => ({
  CustomAppsService: { listSecrets: vi.fn(), moveProduction: vi.fn() }
}));
vi.mock("@/hooks/api/customApps/useAppLifecycle", () => ({
  useAppLifecycle: () => LIFECYCLE
}));
vi.mock("./actions", () => ({
  useAdminLifecycleActions: () => ({
    promoteDraft: vi.fn(async () => undefined),
    publish: vi.fn(async () => undefined),
    rollbackTo: vi.fn(async () => undefined),
    unpublish: vi.fn(async () => undefined),
    pending: false
  }),
  usePartnerLifecycleActions: vi.fn()
}));

import { AppLifecycle } from "./index";

const build = (over: Partial<AppBuild>): AppBuild => ({
  id: over.id ?? "b",
  build_id: over.build_id ?? "0123456789abcdef",
  created_at: "2026-09-19T17:00:00Z",
  is_draft: false,
  is_published: false,
  ...over
});

const LIVE = build({ id: "live", build_id: "aaaaaaaaaaaaaaaa", is_published: true });
const DRAFT = build({ id: "draft", build_id: "bbbbbbbbbbbbbbbb", is_draft: true });
const OLD = build({ id: "old", build_id: "cccccccccccccccc" });

const LIFECYCLE: Lifecycle = {
  liveBuild: LIVE,
  draftBuild: DRAFT,
  draftAhead: true,
  published: true,
  publishedAt: "2026-09-19T17:30:27Z",
  verdict: lifecycleVerdict({ published: true, health: "operational" }),
  heldCount: null,
  heldTruncated: false,
  lastPromoted: { at: null, byEmail: null },
  builds: [DRAFT, LIVE, OLD],
  isLoading: false,
  error: null
};

const plan = (over: Partial<MovePlan> = {}): MovePlan => ({
  action: "promote",
  app_id: "app-1",
  from: "staging",
  to: "production",
  build: { id: "draft", build_id: "bbbbbbbbbbbbbbbb", published_by: null, created_at: "" },
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

const renderCard = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AppLifecycle app={{ id: "app-1", org_slug: "acme", slug: "storefront" } as CustomApp} />
    </QueryClientProvider>
  );

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AppLifecycle — a production move confirms on its plan", () => {
  it("reads the draft's promote plan from staging and says what it changes", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue(
      plan({
        functions: { added: ["export"], removed: [], changed: ["totals"] },
        schedules: {
          added: [],
          removed: [{ function: "nightly", cron: "0 0 * * *", timezone: "UTC" }],
          changed: []
        },
        migrations: { when: "before_move", oltp: ["0002_note.sql"], airhouse: [] },
        blockers: [{ code: "schedule_removal_unconfirmed", message: "nightly is retired" }]
      })
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    const summary = await within(dialog).findByTestId("app-lifecycle-plan");
    expect(CustomAppsService.moveProduction).toHaveBeenCalledWith(
      "app-1",
      { act: "promote", buildId: "draft" },
      { dry_run: true }
    );
    expect(summary).toHaveTextContent("Retires the schedule of nightly, 0 0 * * * (UTC).");
    expect(summary).toHaveTextContent("Applies 1 migration before it goes live: 0002_note.sql.");
    expect(summary).toHaveTextContent("Adds export.");
    expect(summary).toHaveTextContent("Changes totals.");
    // A schedule removal is the one blocker the confirm itself confirms.
    expect(within(dialog).queryByTestId(/app-lifecycle-plan-blocker-/)).toBeNull();
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
  });

  it("holds the verb on a blocker oxyc promote would refuse on", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue(
      plan({
        required_secrets_unset: ["NEW_KEY"],
        blockers: [{ code: "required_secrets_unset", message: "production has no NEW_KEY" }]
      })
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    const blocker = await within(dialog).findByTestId(
      "app-lifecycle-plan-blocker-required_secrets_unset"
    );
    expect(blocker).toHaveTextContent("production has no NEW_KEY");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeDisabled();
  });

  it("holds the verb until the plan answers", async () => {
    let answer: (p: MovePlan) => void = () => {};
    vi.mocked(CustomAppsService.moveProduction).mockReturnValue(
      new Promise((resolve) => (answer = resolve))
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan-loading");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeDisabled();
    answer(plan());
    await within(dialog).findByText("No function, schedule or migration changes.");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
  });

  it("does not move unplanned when the plan cannot be read, and does not retry it", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockRejectedValue(new Error("502"));
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan-error");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeDisabled();
    expect(CustomAppsService.moveProduction).toHaveBeenCalledTimes(1);
  });

  it("reads the rollback plan of the build a rollback picks", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue(
      plan({ action: "rollback", from: null, warnings: ["production has no OLD_KEY"] })
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-rollback"));
    await userEvent.click(await screen.findByTestId("app-lifecycle-rollback-ccccccc"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    const warning = await within(dialog).findByTestId("app-lifecycle-plan-warning");
    expect(warning).toHaveTextContent("production has no OLD_KEY");
    expect(CustomAppsService.moveProduction).toHaveBeenCalledWith(
      "app-1",
      { act: "rollback", buildId: "old" },
      { dry_run: true }
    );
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
  });

  it("asks nothing for a verb that puts no build live", async () => {
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-unpublish"));
    expect(screen.getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
    expect(CustomAppsService.moveProduction).not.toHaveBeenCalled();
    expect(CustomAppsService.listSecrets).not.toHaveBeenCalled();
  });
});
