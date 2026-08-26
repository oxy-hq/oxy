// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Schedule } from "@/types/schedule";
import ScheduleDialog from "./ScheduleDialog";

// A schedule's `variables` decide what its target actually computes, and the
// backend PATCH is a whole-row write — `variables: Set(input.variables)` — so
// anything this form fails to send is CLEARED, not left alone. These pin the
// send path rather than the rendering, because that is where the damage is.

// Mounting a shadcn Dialog under jsdom is slow enough that the 5s default
// trips on the typing tests even with the keystroke delay off.
vi.setConfig({ testTimeout: 20000 });

// Radix's Select reaches for pointer-capture and scrollIntoView, neither of
// which jsdom implements. Without them the trigger click throws instead of
// opening the listbox.
beforeAll(() => {
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});

const createMut = vi.fn();
const updateMut = vi.fn();

vi.mock("@/hooks/api/agentic-automations/useAgenticAutomations", () => ({
  useAgenticAutomationFiles: () => ({ data: [{ path: "je.procedure.yml" }] })
}));
vi.mock("@/hooks/api/schedules/useSchedules", () => ({
  useAirwayFiles: () => ({ data: [] }),
  useScheduleAgents: () => ({ data: [] }),
  useCreateSchedule: () => ({ mutateAsync: createMut, isPending: false }),
  useUpdateSchedule: () => ({ mutateAsync: updateMut, isPending: false })
}));
// CronBuilder owns its own cron parsing; irrelevant here and heavy to mount.
vi.mock("./CronBuilder", () => ({ default: () => null }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

afterEach(() => cleanup());
beforeEach(() => {
  createMut.mockReset().mockResolvedValue({});
  updateMut.mockReset().mockResolvedValue({});
});

const base: Schedule = {
  id: "s1",
  project_id: "p1",
  branch_id: null,
  name: "Rebuild JE",
  target_kind: "workflow",
  target_ref: "je.procedure.yml",
  question: null,
  variables: null,
  cron_expr: "0 */12 * * *",
  timezone: "UTC",
  enabled: true,
  next_run_at: "2026-08-27T00:00:00Z",
  last_fired_at: null,
  last_run_id: null,
  last_error: null,
  missed_runs: 0,
  last_missed_at: null,
  created_at: "2026-08-26T00:00:00Z",
  updated_at: "2026-08-26T00:00:00Z"
};

/** `delay: null` drops userEvent's inter-keystroke wait — without it a
 *  20-character JSON body alone costs seconds. */
const wrap = (schedule: Schedule | null) => {
  const user = userEvent.setup({ delay: null });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ScheduleDialog open onOpenChange={() => {}} schedule={schedule} />
    </QueryClientProvider>
  );
  return {
    user,
    save: () => user.click(screen.getByTestId("coordinator-schedule-save")),
    box: () => screen.getByLabelText("Variables"),
    /** userEvent.type() reads `{` and `[` as keyboard-descriptor syntax. */
    lit: (text: string) => text.replace(/[{[]/g, (c) => c + c),
    /** The Select triggers have no accessible name, so find them by the
     *  value they currently show — stabler than indexing the three. */
    pick: async (showing: string, option: string) => {
      const trigger = screen.getAllByRole("combobox").find((el) => el.textContent === showing);
      if (!trigger) throw new Error(`no combobox showing ${JSON.stringify(showing)}`);
      await user.click(trigger);
      await user.click(await screen.findByRole("option", { name: option }));
    }
  };
};

const sentVariables = () => updateMut.mock.calls[0][0].input.variables;

describe("ScheduleDialog variables", () => {
  it("sends what the operator typed, not the prefilled value", async () => {
    const t = wrap({ ...base, variables: { lookback_days: 30 } });
    await t.user.clear(t.box());
    await t.user.type(t.box(), t.lit('{"lookback_days": 3}'));
    await t.save();
    expect(sentVariables()).toEqual({ lookback_days: 3 });
  });

  it("round-trips existing variables on edit, unchanged", async () => {
    const t = wrap({ ...base, variables: { lookback_days: 3 } });
    await t.save();
    expect(updateMut).toHaveBeenCalledTimes(1);
    expect(sentVariables()).toEqual({ lookback_days: 3 });
  });

  it("sends null when the field is left empty", async () => {
    const t = wrap(base);
    await t.save();
    expect(sentVariables()).toBeNull();
  });

  it("PRESERVES variables for a target kind whose editor is not rendered", async () => {
    // The regression this file exists for. A monitor_scan keeps `granularity`
    // in `variables` and the dialog shows it read-only — so before this field
    // existed, saving any unrelated edit dropped it and the scan then failed
    // every fire with "missing granularity in variables".
    const t = wrap({ ...base, target_kind: "monitor_scan", variables: { granularity: "day" } });
    expect(screen.queryByLabelText("Variables")).toBeNull();
    await t.save();
    expect(sentVariables()).toEqual({ granularity: "day" });
  });

  it("refuses malformed JSON instead of silently sending nothing", async () => {
    const t = wrap({ ...base, variables: { lookback_days: 3 } });
    await t.user.clear(t.box());
    await t.user.type(t.box(), t.lit("{not json"));
    await t.save();
    expect(updateMut).not.toHaveBeenCalled();
  });

  it("sends variables on CREATE too, not only on update", async () => {
    // The claim is "every save". Without this, `createMut` is mocked, reset,
    // and never read — the headline is pinned for one of the two paths.
    const t = wrap(null);
    await t.user.type(screen.getByLabelText("Name"), "New job");
    await t.pick("Select a file", "je.procedure.yml");
    await t.user.type(t.box(), t.lit('{"lookback_days": 3}'));
    await t.save();
    expect(createMut).toHaveBeenCalledTimes(1);
    expect(createMut.mock.calls[0][0].variables).toEqual({ lookback_days: 3 });
  });

  it("drops a half-typed body when the target kind changes", async () => {
    // Otherwise parseVariables keeps failing on text the editor no longer
    // shows, and the save is blocked with no field to correct.
    const t = wrap(null);
    await t.user.type(t.box(), t.lit("{not json"));
    await t.pick("DAG automation", "Agent");
    expect(screen.queryByLabelText("Variables")).toBeNull();
    // Back to a kind that renders it: the stale text must be gone.
    await t.pick("Agent", "DAG automation");
    expect((t.box() as HTMLTextAreaElement).value).toBe("");
  });

  it.each(["[1, 2]", '"a string"', "42"])(
    "refuses %s — the backend folds key by key and could not fold this",
    async (bad) => {
      const t = wrap(base);
      await t.user.clear(t.box());
      await t.user.type(t.box(), t.lit(bad));
      await t.save();
      expect(updateMut).not.toHaveBeenCalled();
    }
  );
});
