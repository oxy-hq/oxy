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

// The real verbs, not a mock of `./actions`: what Confirm sends is the point.
vi.mock("@/services/api/customApps", () => ({
  CustomAppsService: { listSecrets: vi.fn(), moveProduction: vi.fn() }
}));
vi.mock("@/hooks/api/customApps/useAppLifecycle", () => ({ useAppLifecycle: () => LIFECYCLE }));

import { AppLifecycle } from "./index";

const build = (over: Partial<AppBuild>): AppBuild => ({
  id: "b",
  build_id: "0123456789abcdef",
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

const nightly = { function: "nightly", cron: "0 0 * * *", timezone: "UTC" };

const renderCard = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AppLifecycle app={{ id: "app-1", org_slug: "acme", slug: "storefront" } as CustomApp} />
    </QueryClientProvider>
  );

/** The `moveProduction` calls that acted, not the dry runs. */
const acts = () =>
  vi.mocked(CustomAppsService.moveProduction).mock.calls.filter(([, , body]) => !body.dry_run);
const dryRuns = () =>
  vi.mocked(CustomAppsService.moveProduction).mock.calls.filter(([, , body]) => body.dry_run);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AppLifecycle — Confirm acts on the plan the confirm showed", () => {
  it("promotes the draft with what production served and the schedules it retires", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) =>
      body.dry_run
        ? plan({ schedules: { added: [], removed: [nightly], changed: [] } })
        : plan({ dry_run: false, moved: true })
    );
    renderCard();
    // The verb is stored by this click, before the plan has loaded.
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan");
    await userEvent.click(within(dialog).getByTestId("app-lifecycle-confirm-ok"));
    await vi.waitFor(() => expect(acts()).toHaveLength(1));
    expect(acts()[0]).toEqual([
      "app-1",
      { act: "promote", buildId: "draft" },
      { expect_current: "live", confirm_schedule_removals: ["nightly"] }
    ]);
    await vi.waitFor(() => expect(screen.queryByTestId("app-lifecycle-confirm")).toBeNull());
  });

  it("rolls back to the build the confirm planned", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) =>
      plan({ action: "rollback", from: null, dry_run: !!body.dry_run })
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-rollback"));
    await userEvent.click(await screen.findByTestId("app-lifecycle-rollback-ccccccc"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan");
    await userEvent.click(within(dialog).getByTestId("app-lifecycle-confirm-ok"));
    await vi.waitFor(() => expect(acts()).toHaveLength(1));
    expect(acts()[0][1]).toEqual({ act: "rollback", buildId: "old" });
  });

  it("re-reads the plan when the act is refused, and keeps the confirm open", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) => {
      if (body.dry_run) return plan();
      throw new Error("production moved since the plan was made");
    });
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan");
    expect(dryRuns()).toHaveLength(1);
    await userEvent.click(within(dialog).getByTestId("app-lifecycle-confirm-ok"));
    await vi.waitFor(() => expect(dryRuns()).toHaveLength(2));
    expect(screen.getByTestId("app-lifecycle-confirm")).toBeInTheDocument();
  });
  it("holds Confirm when the re-read after a refused act fails", async () => {
    let reads = 0;
    vi.mocked(CustomAppsService.moveProduction).mockImplementation(async (_id, _t, body) => {
      if (body.dry_run) {
        reads += 1;
        if (reads > 1) throw new Error("plan unavailable");
        return plan();
      }
      throw new Error("production moved since the plan was made");
    });
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan");
    await userEvent.click(within(dialog).getByTestId("app-lifecycle-confirm-ok"));
    await within(dialog).findByTestId("app-lifecycle-plan-error");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeDisabled();
    expect(acts()).toHaveLength(1);
  });
});
