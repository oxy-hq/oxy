// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppReleasesService } from "@/services/api/appReleases";
import { CustomAppsService } from "@/services/api/customApps";
import type { MovePlan } from "@/types/appPromote";
import type { CustomApp } from "@/types/apps";
import { release, timeline } from "./fixture";
import { Releases } from "./index";

vi.mock("@/services/api/appReleases", () => ({ AppReleasesService: { list: vi.fn() } }));
vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { moveProduction: vi.fn() } }));

const APP = { id: "app-1", org_slug: "local", slug: "pulse-demo" } as CustomApp;

const plan = (over: Partial<MovePlan> = {}): MovePlan => ({
  action: "rollback",
  app_id: "app-1",
  from: null,
  to: "production",
  build: { id: "build-3", build_id: "pulse-r3-0123456789ab", published_by: null, created_at: "" },
  current: { id: "build-4", build_id: "pulse-r4-0123456789ab" },
  expect_current: "build-4",
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

const renderReleases = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <Releases app={APP} onOpenFunction={vi.fn()} />
    </QueryClientProvider>
  );

const acts = () =>
  vi.mocked(CustomAppsService.moveProduction).mock.calls.filter(([, , body]) => !body.dry_run);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Releases", () => {
  it("shows a red release with why, and Roll back on it alone", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(timeline());
    renderReleases();
    const verdict = await screen.findByTestId("admin-app-release-1-verdict");
    expect(verdict).toHaveAttribute("data-verdict", "worse");
    expect(verdict).toHaveTextContent("Failure rate 0.00% → 16.96%");
    expect(screen.getByTestId("admin-app-release-1-rollback")).toHaveTextContent("pulse-r");
    expect(screen.queryByTestId("admin-app-release-0-rollback")).toBeNull();
    // Vitals per screen, beside the release before.
    const screenRow = screen.getByTestId("admin-app-release-1-vitals");
    expect(within(screenRow).getByText("/store")).toBeInTheDocument();
    expect(within(screenRow).getByText("2,618 ms")).toBeInTheDocument();
    expect(within(screenRow).getByText("+1,318 ms")).toBeInTheDocument();
  });

  it("rolls back through the plan: held until it is read, then acted with what it read", async () => {
    let resolvePlan: (p: MovePlan) => void = () => undefined;
    vi.mocked(AppReleasesService.list).mockResolvedValue(timeline());
    vi.mocked(CustomAppsService.moveProduction).mockImplementation((_id, _t, body) =>
      body.dry_run
        ? new Promise<MovePlan>((r) => {
            resolvePlan = r;
          })
        : Promise.resolve(plan({ dry_run: false, moved: true }))
    );
    renderReleases();
    await userEvent.click(await screen.findByTestId("admin-app-release-1-rollback"));
    const dialog = screen.getByTestId("admin-app-releases-rollback-confirm");
    expect(within(dialog).getByTestId("admin-app-releases-rollback-confirm-ok")).toBeDisabled();
    resolvePlan(plan());
    await within(dialog).findByTestId("app-lifecycle-plan");
    const ok = within(dialog).getByTestId("admin-app-releases-rollback-confirm-ok");
    await vi.waitFor(() => expect(ok).toBeEnabled());
    await userEvent.click(ok);
    await vi.waitFor(() => expect(acts()).toHaveLength(1));
    expect(acts()[0]).toEqual([
      "app-1",
      { act: "rollback", buildId: "build-3" },
      { expect_current: "build-4", confirm_schedule_removals: [] }
    ]);
  });

  it("keeps Roll back held while the plan has a blocker", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(timeline());
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue(
      plan({ blockers: [{ code: "validation_failed", message: "the build failed validation" }] })
    );
    renderReleases();
    await userEvent.click(await screen.findByTestId("admin-app-release-1-rollback"));
    const dialog = screen.getByTestId("admin-app-releases-rollback-confirm");
    await within(dialog).findByTestId("app-lifecycle-plan-blocker-validation_failed");
    expect(within(dialog).getByTestId("admin-app-releases-rollback-confirm-ok")).toBeDisabled();
  });

  it("hides staging and sandboxes without reach, and says so", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(
      timeline({ non_production: "not_permitted" })
    );
    renderReleases();
    await screen.findByTestId("admin-app-releases-nonprod-hidden");
    expect(screen.getByTestId("admin-app-releases-lane-production")).toBeInTheDocument();
    expect(screen.queryByTestId("admin-app-releases-lane-staging")).toBeNull();
    expect(screen.queryByTestId("admin-app-releases-lane-sandboxes")).toBeNull();
  });

  it("says metrics could not be read rather than showing zeros", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(
      timeline({
        parts: { metrics: "unavailable", vitals: "ok", migrations: "ok", issues: "ok" },
        releases: [release({ metrics: null })]
      })
    );
    renderReleases();
    const metrics = await screen.findByTestId("admin-app-release-0-metrics");
    expect(metrics).toHaveTextContent("Couldn't measure this release's function calls.");
    expect(metrics).not.toHaveTextContent("0.00%");
  });

  it("says a migrations or new-failures read was unavailable rather than showing none", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(
      timeline({
        parts: { metrics: "ok", vitals: "ok", migrations: "unavailable", issues: "unavailable" }
      })
    );
    renderReleases();
    await screen.findByTestId("admin-app-releases-migrations-unavailable");
    expect(screen.getByTestId("admin-app-releases-issues-unavailable")).toHaveTextContent(
      "Couldn't read which failures are new."
    );
  });

  it("says nothing about parts that were read", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(timeline());
    renderReleases();
    await screen.findByTestId("admin-app-release-1");
    expect(screen.queryByTestId("admin-app-releases-migrations-unavailable")).toBeNull();
    expect(screen.queryByTestId("admin-app-releases-issues-unavailable")).toBeNull();
  });

  it("shows a release serving since before the window even with no move in it", async () => {
    const quiet = timeline({
      lanes: [
        { lane: "production", events: [] },
        { lane: "staging", events: [] },
        { lane: "sandboxes", events: [] }
      ],
      releases: [release({ current: true, clipped: true, ended_at: null })]
    });
    vi.mocked(AppReleasesService.list).mockResolvedValue(quiet);
    renderReleases();
    await screen.findByTestId("admin-app-release-0");
    expect(screen.queryByTestId("admin-app-releases-empty")).toBeNull();
  });

  it("reads the window the operator picks", async () => {
    vi.mocked(AppReleasesService.list).mockResolvedValue(timeline());
    renderReleases();
    await screen.findByTestId("admin-app-release-1");
    expect(AppReleasesService.list).toHaveBeenLastCalledWith("app-1", 7);
    await userEvent.click(screen.getByTestId("admin-app-releases-window-30"));
    await vi.waitFor(() => expect(AppReleasesService.list).toHaveBeenLastCalledWith("app-1", 30));
    expect(screen.getByTestId("admin-app-releases-window-30")).toHaveAttribute(
      "aria-pressed",
      "true"
    );
  });
});
