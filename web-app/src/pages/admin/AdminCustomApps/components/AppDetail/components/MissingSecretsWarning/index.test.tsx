// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { AppSecrets } from "@/types/apps";

vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { listSecrets: vi.fn() } }));

import { MissingSecretsWarning } from "./index";

const listSecrets = vi.mocked(CustomAppsService.listSecrets);

/** Production's secrets reconciled against the build being promoted. */
const forBuild: AppSecrets = {
  app_id: "app-1",
  app_slug: "storefront",
  app_name: "Storefront",
  environment: "production",
  missing_required: 2,
  declaring_build_id: "b2",
  entries: [
    { key: "NEW_KEY", is_set: false, declared: true, required: true, source: "manifest" },
    { key: "WEBHOOK_SECRET", is_set: false, declared: true, required: true, source: "webhook" },
    { key: "OPTIONAL", is_set: false, declared: true, required: false, source: "manifest" },
    { key: "STRIPE_KEY", is_set: true, declared: true, required: true, source: "manifest" }
  ]
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const renderWarning = (buildId: string | undefined) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MissingSecretsWarning appId='app-1' buildId={buildId} />
    </QueryClientProvider>
  );

describe("MissingSecretsWarning", () => {
  it("names the required keys the build declares that production lacks", async () => {
    listSecrets.mockResolvedValue(forBuild);
    renderWarning("build-2");
    const warning = await screen.findByTestId("admin-app-missing-secrets-warning");
    expect(warning).toHaveTextContent(
      "This build declares required secrets that production does not have: NEW_KEY, WEBHOOK_SECRET."
    );
    expect(listSecrets).toHaveBeenCalledWith("app-1", "production", "build-2");
  });

  it("says nothing when the build would find everything it requires", async () => {
    listSecrets.mockResolvedValue({
      ...forBuild,
      missing_required: 0,
      entries: forBuild.entries.filter((e) => e.is_set || !e.required)
    });
    renderWarning("build-2");
    await vi.waitFor(() => expect(listSecrets).toHaveBeenCalled());
    expect(screen.queryByTestId("admin-app-missing-secrets-warning")).toBeNull();
  });

  it("says it is still checking, rather than nothing, until the answer arrives", async () => {
    let answer: (s: AppSecrets) => void = () => {};
    listSecrets.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    renderWarning("build-2");
    expect(await screen.findByTestId("admin-app-missing-secrets-checking")).toHaveTextContent(
      "Checking required secrets…"
    );
    answer(forBuild);
    await screen.findByTestId("admin-app-missing-secrets-warning");
    expect(screen.queryByTestId("admin-app-missing-secrets-checking")).toBeNull();
  });

  it("asks nothing without a build to promote", () => {
    renderWarning(undefined);
    expect(listSecrets).not.toHaveBeenCalled();
    expect(screen.queryByTestId("admin-app-missing-secrets-warning")).toBeNull();
  });
});
