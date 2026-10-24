// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { AppAvailability, AppAvailabilityReading } from "@/types/apps";
import { Availability } from ".";

vi.mock("@/services/api/customApps", () => ({
  CustomAppsService: { availability: vi.fn(), list: vi.fn() }
}));

const reading: AppAvailabilityReading = {
  verdict: "no_opinion",
  objective: 0.99,
  windows: [{ window_minutes: 5, total: 0, failed: 0, failure_ratio: null }],
  open_builds: {
    status: "none_seen",
    live_sessions: 0,
    older_sessions: 0,
    older_builds: 0,
    window_minutes: 1440
  }
};

const renderPanel = (availability?: AppAvailabilityReading) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <Availability app={{ id: "app-1", org_slug: "acme", slug: "store", availability }} />
    </QueryClientProvider>
  );

describe("Availability panel", () => {
  beforeEach(() => {
    vi.mocked(CustomAppsService.list).mockResolvedValue({ items: [], next_offset: null });
  });
  afterEach(() => {
    cleanup();
    vi.resetAllMocks();
  });

  it("renders the reading the list row carries without asking the app's endpoint", () => {
    renderPanel(reading);

    expect(screen.getByTestId("admin-app-availability-verdict-pill").textContent).toMatch(
      /no data|no opinion/i
    );
    expect(screen.getByTestId("admin-app-open-builds")).toBeTruthy();
    expect(CustomAppsService.availability).not.toHaveBeenCalled();
  });

  it("asks the endpoint when the row carries nothing", async () => {
    vi.mocked(CustomAppsService.availability).mockResolvedValue({
      ...reading,
      app_id: "app-1",
      org_slug: "acme",
      app_slug: "store"
    } satisfies AppAvailability);

    renderPanel(undefined);

    await waitFor(() => expect(screen.getByTestId("admin-app-availability")).toBeTruthy());
    expect(CustomAppsService.availability).toHaveBeenCalledWith("acme", "store");
  });
});
