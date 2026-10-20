// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { AppSecretEntry } from "@/types/apps";

vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { copySecret: vi.fn() } }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { CopyFrom, copySources, offeredSources } from "./CopyFrom";

const entry: AppSecretEntry = {
  key: "QB_TOKEN",
  is_set: false,
  declared: true,
  required: true,
  source: "manifest"
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/** `productionHolds`: whether production stores the key — the pane reads it
 *  from production's own view. Most cases here are about a key it does hold. */
const renderFor = (environment: string, e: AppSecretEntry = entry, productionHolds = true) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CopyFrom
        appId='app-1'
        environment={environment}
        entry={e}
        productionHolds={productionHolds}
      />
    </QueryClientProvider>
  );

describe("CopyFrom", () => {
  it("copies only downhill: nothing into production", () => {
    expect(copySources("production")).toEqual([]);
    expect(copySources("staging")).toEqual(["production"]);
    expect(copySources("dev-luong")).toEqual(["staging", "production"]);
  });

  it("asks the server to copy, so the value never reaches the browser", async () => {
    vi.mocked(CustomAppsService.copySecret).mockResolvedValue("created");
    renderFor("staging");
    fireEvent.click(screen.getByRole("button", { name: "Copy from production" }));
    await waitFor(() =>
      expect(CustomAppsService.copySecret).toHaveBeenCalledWith(
        "app-1",
        "QB_TOKEN",
        "staging",
        "production"
      )
    );
    await waitFor(() =>
      expect(vi.mocked(toast.success).mock.calls).toEqual([["Copied QB_TOKEN from production."]])
    );
  });

  it("offers staging and production to a sandbox, and nothing in production", () => {
    renderFor("dev-luong", { ...entry, inherits_staging: true });
    expect(screen.getByRole("button", { name: "Copy from staging" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy from production" })).toBeInTheDocument();
    cleanup();
    renderFor("production");
    expect(screen.queryByRole("button", { name: /Copy from/ })).toBeNull();
  });

  it("never offers to overwrite a value the environment already holds", () => {
    // Staging's own refresh token for a sandbox company: one click must not
    // replace it with production's.
    expect(offeredSources("staging", { ...entry, is_set: true }, true)).toEqual([]);
    expect(offeredSources("dev-luong", { ...entry, is_set: true }, true)).toEqual([]);
    renderFor("staging", { ...entry, is_set: true });
    expect(screen.queryByRole("button", { name: /Copy from/ })).toBeNull();
  });

  it("offers staging to a sandbox only when staging stores the key", () => {
    // A shared key the sandbox reads from production: staging holds no row.
    const shared = { ...entry, shared: true, inherits_production: true };
    expect(offeredSources("dev-luong", shared, true)).toEqual(["production"]);
    expect(offeredSources("dev-luong", { ...entry, inherits_staging: true }, true)).toEqual([
      "staging",
      "production"
    ]);
    renderFor("dev-luong", shared);
    expect(screen.queryByRole("button", { name: "Copy from staging" })).toBeNull();
    expect(screen.getByRole("button", { name: "Copy from production" })).toBeInTheDocument();
  });

  it("offers production only when production holds the key, since a copy of nothing can only 404", () => {
    expect(offeredSources("staging", entry, false)).toEqual([]);
    expect(offeredSources("dev-luong", { ...entry, inherits_staging: true }, false)).toEqual([
      "staging"
    ]);
    // Not yet known (production's view still loading, or it failed): not offered.
    expect(offeredSources("dev-luong", entry)).toEqual([]);
    renderFor("dev-luong", entry, false);
    expect(screen.queryByRole("button", { name: /Copy from/ })).toBeNull();
    cleanup();
    renderFor("dev-luong", { ...entry, inherits_staging: true }, false);
    expect(screen.getByRole("button", { name: "Copy from staging" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Copy from production" })).toBeNull();
  });

  it("hints that a declared key can be shared instead, unless it already is", () => {
    renderFor("staging");
    expect(screen.getByText(/to read production/)).toBeInTheDocument();
    cleanup();
    renderFor("staging", { ...entry, shared: true });
    expect(screen.queryByText(/to read production/)).toBeNull();
  });
});
