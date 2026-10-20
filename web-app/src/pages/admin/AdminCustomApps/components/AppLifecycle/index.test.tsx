// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppLifecycle as Lifecycle } from "@/hooks/api/customApps/deriveLifecycle";
import { CustomAppsService } from "@/services/api/customApps";
import type { AppBuild, AppSecrets, CustomApp } from "@/types/apps";
import { lifecycleVerdict } from "../../appStatus";

vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { listSecrets: vi.fn() } }));
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

const missing = (key: string): AppSecrets => ({
  app_id: "app-1",
  app_slug: "storefront",
  app_name: "Storefront",
  environment: "production",
  missing_required: 1,
  entries: [{ key, is_set: false, declared: true, required: true, source: "manifest" }]
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

describe("AppLifecycle — required secrets before a build goes live", () => {
  it("warns in the promote confirm, and says oxyc promote would refuse it", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(missing("NEW_KEY"));
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    const warning = await within(dialog).findByTestId("admin-app-missing-secrets-warning");
    expect(warning).toHaveTextContent("production does not have: NEW_KEY");
    expect(warning).toHaveTextContent("oxyc promote refuses this build until they are set.");
    expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "production", "draft");
    // A warning, not a gate: the console's promote does not refuse it.
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
  });

  it("frees the confirm on the first failed check rather than retrying it", async () => {
    // A rollback during an outage of the secrets list must not wait out retries:
    // a warning that cannot be computed is not shown, so retrying only delays it.
    vi.mocked(CustomAppsService.listSecrets).mockRejectedValue(new Error("502"));
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await vi.waitFor(
      () => expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled(),
      { timeout: 500 }
    );
    expect(CustomAppsService.listSecrets).toHaveBeenCalledTimes(1);
  });

  it("holds the confirm until the check settles, then never blocks", async () => {
    let answer: (s: AppSecrets) => void = () => {};
    vi.mocked(CustomAppsService.listSecrets).mockReturnValue(
      new Promise((resolve) => (answer = resolve))
    );
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-promote"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    await within(dialog).findByText("Checking required secrets…");
    // Clicked within the request's round trip, the warning would be skipped.
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeDisabled();
    answer(missing("NEW_KEY"));
    await within(dialog).findByTestId("admin-app-missing-secrets-warning");
    expect(within(dialog).getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
  });

  it("checks the build a rollback picks, and only warns", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(missing("OLD_KEY"));
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-rollback"));
    await userEvent.click(await screen.findByTestId("app-lifecycle-rollback-ccccccc"));
    const dialog = screen.getByTestId("app-lifecycle-confirm");
    const warning = await within(dialog).findByTestId("admin-app-missing-secrets-warning");
    expect(warning).toHaveTextContent("production does not have: OLD_KEY");
    expect(warning).not.toHaveTextContent("oxyc promote");
    expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "production", "old");
  });

  it("asks nothing for a verb that puts no build live", async () => {
    renderCard();
    await userEvent.click(screen.getByTestId("app-lifecycle-unpublish"));
    expect(screen.getByTestId("app-lifecycle-confirm-ok")).toBeEnabled();
    expect(CustomAppsService.listSecrets).not.toHaveBeenCalled();
  });
});
