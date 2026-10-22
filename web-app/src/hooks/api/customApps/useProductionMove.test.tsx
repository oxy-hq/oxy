// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { MovePlan } from "@/types/appPromote";
import { hardBlockers, useMoveProduction } from "./useProductionMove";

vi.mock("@/services/api/customApps", () => ({ CustomAppsService: { moveProduction: vi.fn() } }));

const nightly = { function: "nightly", cron: "0 0 * * *", timezone: "UTC" };

const plan = {
  expect_current: "live",
  schedules: { added: [], removed: [nightly], changed: [] },
  blockers: [
    { code: "schedule_removal_unconfirmed", message: "nightly is retired" },
    { code: "required_secrets_unset", message: "production has no NEW_KEY" }
  ]
} as unknown as MovePlan;

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

describe("useMoveProduction", () => {
  it("acts on the plan it was shown: what production served, and the schedules it retires", async () => {
    vi.mocked(CustomAppsService.moveProduction).mockResolvedValue({ moved: true } as MovePlan);
    const { result } = renderHook(() => useMoveProduction("app-1"), { wrapper });
    const target = { act: "promote", buildId: "draft" } as const;
    await result.current.mutateAsync({ target, plan });
    expect(CustomAppsService.moveProduction).toHaveBeenCalledWith("app-1", target, {
      expect_current: "live",
      confirm_schedule_removals: ["nightly"]
    });
  });
});

describe("hardBlockers", () => {
  it("leaves out only the schedule removal the confirm itself confirms", () => {
    expect(hardBlockers(plan).map((b) => b.code)).toEqual(["required_secrets_unset"]);
    expect(hardBlockers(undefined)).toEqual([]);
  });
});
