import { describe, expect, it } from "vitest";
import { buildEltModel } from "@/pages/ide/coordinator/Runs/RunDetail/components/Elt/model";
import type { AirwayEvent } from "@/services/api/airway";
import type { RunEventEntry } from "@/services/api/coordinator";
import { deriveFailedTables } from "./airwayFailedTables";
import { reduceAirwayEvents } from "./airwayReducer";

// One event script, replayed through all three consumers: the shared
// derivation, the IDE pipeline page's reducer, and the coordinator run
// detail's model. The two "Re-run failed tables" surfaces must agree.

const PIPE = "toast";
const LOAD = "load-1";
type Ev = { type: string; payload: Record<string, unknown> };

const loadStarted = (load_id = LOAD): Ev => ({
  type: "load_started",
  payload: { pipeline_name: PIPE, load_id }
});
const plan = (resources: string[]): Ev => ({
  type: "pipeline_plan",
  payload: { pipeline_name: PIPE, load_id: LOAD, resources, destination: "airhouse" }
});
const tableLoaded = (table: string, rows = 10): Ev => ({
  type: "table_loaded",
  payload: { pipeline_name: PIPE, load_id: LOAD, table, rows }
});
const tableLoadFailed = (table: string, error: string): Ev => ({
  type: "table_load_failed",
  payload: { pipeline_name: PIPE, load_id: LOAD, table, error }
});
const resourceFailed = (table: string, error: string): Ev => ({
  type: "resource_failed",
  payload: { pipeline_name: PIPE, load_id: LOAD, table, error }
});
const loadCompleted = (rows_loaded: Record<string, number>): Ev => ({
  type: "load_completed",
  payload: {
    pipeline_name: PIPE,
    load_id: LOAD,
    tables: Object.keys(rows_loaded),
    rows_loaded,
    duration_ms: 5,
    folds: []
  }
});
const pipelineError = (error: string): Ev => ({
  type: "pipeline_error",
  payload: { pipeline_name: PIPE, load_id: LOAD, error }
});
const taskFailed = (error: string): Ev => ({ type: "task_failed", payload: { error } });

/** Failed tables as each surface would hand them to the button. */
const surfaces = (events: Ev[], runError: string | null = null) => ({
  shared: deriveFailedTables(events).map((f) => f.table),
  ide: reduceAirwayEvents(events as unknown as AirwayEvent[]).failedResources.map((f) => f.table),
  coordinator: buildEltModel(
    [],
    events.map<RunEventEntry>((e, i) => ({ seq: i, event_type: e.type, payload: e.payload })),
    { runError }
  )
    .tables.filter((t) => t.status === "failed")
    .map((t) => t.name)
});

describe("failed tables for the re-run offer", () => {
  it("phantom: a run failed at the run level with every table loaded offers nothing", () => {
    const events = [
      loadStarted(),
      plan(["orders", "payments"]),
      tableLoaded("orders"),
      tableLoaded("payments"),
      loadCompleted({ orders: 10, payments: 10 }),
      // The run is marked failed after the load finished (e.g. a restart).
      taskFailed("worker lost its lease")
    ];
    expect(surfaces(events, "worker lost its lease")).toEqual({
      shared: [],
      ide: [],
      coordinator: []
    });
    // The run status is failed — and still nothing is offered.
    expect(reduceAirwayEvents(events as unknown as AirwayEvent[]).status).toBe("failed");
  });

  it("phantom: a pipeline_error with no per-table failure offers nothing", () => {
    const events = [
      loadStarted(),
      plan(["orders", "payments"]),
      tableLoaded("orders"),
      pipelineError("connection reset")
    ];
    expect(surfaces(events)).toEqual({ shared: [], ide: [], coordinator: [] });
  });

  it("real: one table_load_failed offers exactly that table", () => {
    const events = [
      loadStarted(),
      plan(["orders", "payments"]),
      tableLoaded("orders"),
      tableLoadFailed("payments", "permission denied"),
      loadCompleted({ orders: 10 })
    ];
    expect(surfaces(events)).toEqual({
      shared: ["payments"],
      ide: ["payments"],
      coordinator: ["payments"]
    });
    expect(deriveFailedTables(events)).toEqual([{ table: "payments", error: "permission denied" }]);
  });

  it("a table that emits both failure events is offered once, with its first error", () => {
    const events = [
      loadStarted(),
      resourceFailed("orders", "HTTP 403"),
      tableLoadFailed("orders", "nothing to load"),
      loadCompleted({})
    ];
    expect(deriveFailedTables(events)).toEqual([{ table: "orders", error: "HTTP 403" }]);
    expect(surfaces(events).coordinator).toEqual(["orders"]);
  });

  it("re-drive: an earlier attempt's failures don't survive a later attempt that loaded", () => {
    const events = [
      // Attempt 1 — cut off mid-load.
      loadStarted("load-1"),
      plan(["orders", "payments"]),
      tableLoadFailed("orders", "connection closed"),
      pipelineError("shutting down"),
      // Attempt 2 — reset-in-place re-drive on the same run id.
      loadStarted("load-2"),
      plan(["orders", "payments"]),
      tableLoaded("orders"),
      tableLoaded("payments"),
      loadCompleted({ orders: 10, payments: 10 })
    ];
    expect(surfaces(events)).toEqual({ shared: [], ide: [], coordinator: [] });
    const view = reduceAirwayEvents(events as unknown as AirwayEvent[]);
    expect(view.status).toBe("done");
    expect(view.error).toBeUndefined();
    expect(view.resources.every((r) => r.status === "done")).toBe(true);
    const model = buildEltModel(
      [],
      events.map<RunEventEntry>((e, i) => ({ seq: i, event_type: e.type, payload: e.payload }))
    );
    expect(model.pipelineError).toBeNull();
  });

  it("re-drive: a failure in the latest attempt is still offered", () => {
    const events = [
      loadStarted("load-1"),
      tableLoadFailed("orders", "old failure"),
      loadStarted("load-2"),
      tableLoaded("orders"),
      tableLoadFailed("payments", "new failure"),
      loadCompleted({ orders: 10 })
    ];
    expect(surfaces(events)).toEqual({
      shared: ["payments"],
      ide: ["payments"],
      coordinator: ["payments"]
    });
  });
});
