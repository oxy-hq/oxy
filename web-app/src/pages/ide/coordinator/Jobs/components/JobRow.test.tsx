// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Table, TableBody } from "@/components/ui/shadcn/table";
import type { Schedule } from "@/types/schedule";
import { JobRow } from "./JobRow";

// `toInput` rebuilds the WHOLE row for a PATCH that overwrites the whole row,
// so every field it forgets is a field the toggle silently drops.

vi.setConfig({ testTimeout: 20000 });

const updateMutate = vi.fn();
vi.mock("@/hooks/api/schedules/useSchedules", () => ({
  useUpdateSchedule: () => ({ mutate: updateMutate, isPending: false }),
  useDeleteSchedule: () => ({ mutate: vi.fn(), isPending: false })
}));
vi.mock("react-router-dom", () => ({ useNavigate: () => vi.fn() }));
vi.mock("../../components/useCoordinatorRoutes", () => ({
  useCoordinatorRoutes: () => ({ JOB_DETAIL: (id: string) => `/jobs/${id}` })
}));
vi.mock("./DeleteJobDialog", () => ({ default: () => null }));
vi.mock("./RunNowDialog", () => ({ default: () => null }));
vi.mock("./ScheduleDialog", () => ({ default: () => null }));

afterEach(() => cleanup());
beforeEach(() => updateMutate.mockReset());

const agentSchedule: Schedule = {
  id: "s1",
  project_id: "p1",
  branch_id: null,
  name: "Morning briefing",
  target_kind: "agent",
  target_ref: "analytics",
  question: "How did sales do yesterday?",
  variables: { tone: "brief" },
  cron_expr: "0 9 * * *",
  timezone: "UTC",
  enabled: true,
  next_run_at: "2026-08-27T09:00:00Z",
  last_fired_at: null,
  last_run_id: null,
  last_error: null,
  missed_runs: 0,
  last_missed_at: null,
  created_at: "2026-08-26T00:00:00Z",
  updated_at: "2026-08-26T00:00:00Z"
};

const renderRow = (schedule: Schedule) => {
  const user = userEvent.setup({ delay: null });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <Table>
        <TableBody>
          <JobRow schedule={schedule} canManage />
        </TableBody>
      </Table>
    </QueryClientProvider>
  );
  return { user, toggle: () => user.click(screen.getByRole("switch")) };
};

describe("JobRow enable/disable toggle", () => {
  it("keeps the agent question, which the backend refuses to see empty", async () => {
    // `validate_input` rejects an empty question for agent schedules, so
    // omitting it here turns a toggle into a 400 — loud, but still the same
    // whole-row-write hazard that made the dialog erase `variables`.
    const t = renderRow(agentSchedule);
    await t.toggle();
    expect(updateMutate).toHaveBeenCalledTimes(1);
    expect(updateMutate.mock.calls[0][0].input.question).toBe("How did sales do yesterday?");
  });

  it("keeps variables too", async () => {
    const t = renderRow(agentSchedule);
    await t.toggle();
    expect(updateMutate.mock.calls[0][0].input.variables).toEqual({ tone: "brief" });
  });

  it("still sends the flipped enabled value", async () => {
    const t = renderRow(agentSchedule);
    await t.toggle();
    expect(updateMutate.mock.calls[0][0].input.enabled).toBe(false);
  });
});
