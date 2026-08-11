import { describe, expect, it } from "vitest";

import type { ContractMutability, ResourceContract } from "@/services/api/airway";
import type { ResourceRow } from "@/utils/airwayReducer";

import { deriveBackfillSuggestion, suggestionRationale } from "./backfillSuggestion";

const DAY = 86_400_000;
/** Mid-afternoon UTC, so a day-floored `fromDate` is visibly a floor. */
const NOW = new Date("2026-03-10T15:30:00.000Z");

/** A contract with everything absent — the `undeclared` shape. */
function contract(over: Partial<ResourceContract> & { mutability: ContractMutability }) {
  return {
    resource: "orders",
    version_field: null,
    version_column: null,
    cursor_tracks_modification: null,
    restatement_window_ms: null,
    cursor_lag_ms: null,
    rewind_ms: null,
    requires_partition_repull: null,
    ...over
  } satisfies ResourceContract;
}

/** A declared, mutable contract that restates within `windowMs`. */
const restating = (resource: string, windowMs: number, partitionRepull = false) =>
  contract({
    resource,
    mutability: "versioned",
    version_field: "modifiedDate",
    version_column: "modified_date",
    cursor_tracks_modification: true,
    restatement_window_ms: windowMs,
    cursor_lag_ms: 30_000,
    rewind_ms: windowMs + 30_000,
    requires_partition_repull: partitionRepull
  });

const row = (table: string, c?: ResourceContract, parent?: string): ResourceRow => ({
  table,
  status: "done",
  ...(c ? { contract: c } : {}),
  ...(parent ? { parent } : {})
});

describe("deriveBackfillSuggestion — the derivation", () => {
  it("suggests `now - restatement_window` → today, floored to UTC days", () => {
    const s = deriveBackfillSuggestion([row("orders", restating("orders", 3 * DAY))], NOW);
    expect(s.window).toEqual({
      fromDate: "2026-03-07",
      toDate: "2026-03-10",
      windowMs: 3 * DAY,
      resource: "orders",
      floorOnly: false
    });
  });

  it("derives from `restatement_window_ms`, never from `rewind_ms`", () => {
    // rewind = cursor_lag + restatement_window, a *cursor-axis* quantity. The
    // backfill window is on the business/event axis, so folding cursor lag in
    // would be an axis mix-up. A 2-day window whose rewind crosses into a
    // third day must still start 2 days back.
    const c = restating("orders", 2 * DAY);
    c.cursor_lag_ms = 20 * 3_600_000; // 20h — pushes `rewind` past 2d
    c.rewind_ms = 2 * DAY + 20 * 3_600_000;
    const s = deriveBackfillSuggestion([row("orders", c)], NOW);
    expect(s.window?.fromDate).toBe("2026-03-08");
    expect(s.window?.windowMs).toBe(2 * DAY);
  });

  it("floors the start to the UTC day, which widens rather than narrows", () => {
    // now - 1d = 2026-03-09T15:30Z; the day floor is 03-09T00:00Z, i.e. 15.5h
    // earlier. Re-reading extra is idempotent; skipping is silent loss.
    const s = deriveBackfillSuggestion([row("orders", restating("orders", DAY))], NOW);
    expect(s.window?.fromDate).toBe("2026-03-09");
  });

  it("keeps a declared zero window as a real statement (today only)", () => {
    const s = deriveBackfillSuggestion([row("orders", restating("orders", 0))], NOW);
    expect(s.window?.fromDate).toBe("2026-03-10");
    expect(s.window?.toDate).toBe("2026-03-10");
  });
});

describe("deriveBackfillSuggestion — absence is unknown, not zero", () => {
  it("suggests nothing when the contract is undeclared", () => {
    const s = deriveBackfillSuggestion(
      [row("orders", contract({ resource: "orders", mutability: "undeclared" }))],
      NOW
    );
    expect(s.window).toBeNull();
    expect(s.declared).toEqual([]);
    expect(s.excluded).toEqual([{ resource: "orders", reason: "undeclared" }]);
  });

  it("suggests nothing when a declared mutable contract names no window", () => {
    // The dominant real case: Toast declares mutability but no widths,
    // deliberately, because a guessed window is how you get a silent gap.
    const declaredButSilent = contract({
      resource: "orders",
      mutability: "versioned",
      version_field: "modifiedDate",
      cursor_tracks_modification: true,
      restatement_window_ms: null,
      cursor_lag_ms: 0,
      rewind_ms: 0,
      requires_partition_repull: false
    });
    const s = deriveBackfillSuggestion([row("orders", declaredButSilent)], NOW);
    expect(s.window).toBeNull();
    expect(s.excluded).toEqual([{ resource: "orders", reason: "no_window" }]);
  });

  it("suggests nothing for an immutable resource — there is nothing to re-pull", () => {
    const s = deriveBackfillSuggestion(
      [
        row(
          "weather_forecast",
          contract({
            resource: "weather_forecast",
            mutability: "immutable",
            cursor_tracks_modification: true,
            cursor_lag_ms: 0,
            rewind_ms: 0,
            requires_partition_repull: false
          })
        )
      ],
      NOW
    );
    expect(s.window).toBeNull();
    expect(s.excluded).toEqual([{ resource: "weather_forecast", reason: "immutable" }]);
  });

  it("still ignores an immutable resource that somehow carries a window", () => {
    // airway refuses `restating_within` on Immutable, so this cannot happen
    // today. If it ever does, an immutable resource must contribute nothing
    // rather than propose re-pulling rows that are never corrected.
    const contradictory = contract({
      resource: "events",
      mutability: "immutable",
      restatement_window_ms: 30 * DAY
    });
    const s = deriveBackfillSuggestion([row("events", contradictory)], NOW);
    expect(s.window).toBeNull();
    expect(s.excluded).toEqual([{ resource: "events", reason: "immutable" }]);
  });

  it("reports a row the run carried no contract for as `unreported`", () => {
    // An old run, recorded before contracts rode on `pipeline_plan`. Nobody
    // was ever asked — distinct from a connector that answered "nothing".
    const s = deriveBackfillSuggestion([row("orders")], NOW);
    expect(s.window).toBeNull();
    expect(s.excluded).toEqual([{ resource: "orders", reason: "unreported" }]);
  });

  it("suggests nothing when every resource is silent for a different reason", () => {
    const s = deriveBackfillSuggestion(
      [
        row("orders", contract({ resource: "orders", mutability: "undeclared" })),
        row("events", contract({ resource: "events", mutability: "immutable" })),
        row("payments")
      ],
      NOW
    );
    expect(s.window).toBeNull();
    expect(s.disagree).toBe(false);
    expect(s.excluded.map((e) => e.reason)).toEqual(["undeclared", "immutable", "unreported"]);
  });

  it("returns an empty suggestion for an empty resource list", () => {
    const s = deriveBackfillSuggestion([], NOW);
    expect(s).toEqual({ window: null, declared: [], excluded: [], disagree: false });
  });
});

describe("deriveBackfillSuggestion — resources that disagree", () => {
  it("takes the widest declared window and names which resource set it", () => {
    const s = deriveBackfillSuggestion(
      [
        row("time_entries", restating("time_entries", DAY)),
        row("orders", restating("orders", 7 * DAY))
      ],
      NOW
    );
    expect(s.window?.resource).toBe("orders");
    expect(s.window?.fromDate).toBe("2026-03-03");
    expect(s.declared.map((d) => d.resource)).toEqual(["orders", "time_entries"]);
    expect(s.disagree).toBe(true);
  });

  it("does not flag disagreement when every declared window matches", () => {
    const s = deriveBackfillSuggestion(
      [row("a", restating("a", 2 * DAY)), row("b", restating("b", 2 * DAY))],
      NOW
    );
    expect(s.disagree).toBe(false);
    // Ties break by name so the disclosure order is stable.
    expect(s.declared.map((d) => d.resource)).toEqual(["a", "b"]);
  });

  it("keeps silent resources listed alongside the ones that spoke", () => {
    const s = deriveBackfillSuggestion(
      [
        row("orders", restating("orders", 3 * DAY)),
        row("menus", contract({ resource: "menus", mutability: "undeclared" }))
      ],
      NOW
    );
    expect(s.window?.windowMs).toBe(3 * DAY);
    expect(s.excluded).toEqual([{ resource: "menus", reason: "undeclared" }]);
  });

  it("marks a partition-repull resource's window as a floor", () => {
    const s = deriveBackfillSuggestion([row("things", restating("things", 5 * DAY, true))], NOW);
    expect(s.window?.floorOnly).toBe(true);
    expect(s.declared[0].floorOnly).toBe(true);
  });
});

describe("deriveBackfillSuggestion — normalized child tables", () => {
  it("skips child rows entirely rather than reporting them as unknown", () => {
    // `orders__checks` is produced by relational normalization, not pulled
    // from the source, so it has no contract of its own and listing it would
    // misdescribe it.
    const s = deriveBackfillSuggestion(
      [
        row("orders", restating("orders", DAY)),
        row("orders__checks", undefined, "orders"),
        row("orders__checks__selections", undefined, "orders__checks")
      ],
      NOW
    );
    expect(s.excluded).toEqual([]);
    expect(s.declared).toHaveLength(1);
  });
});

describe("suggestionRationale", () => {
  it("explains a plain restatement window", () => {
    const s = deriveBackfillSuggestion([row("orders", restating("orders", 7 * DAY))], NOW);
    expect(suggestionRationale(s)).toBe("orders can be corrected for up to 7d after the fact.");
  });

  it("says `at least` when the window is only a floor", () => {
    const s = deriveBackfillSuggestion([row("things", restating("things", 3 * DAY, true))], NOW);
    expect(suggestionRationale(s)).toBe(
      "things is re-pulled whole by partition; its contract keeps partitions open for at least 3d."
    );
  });

  it("has nothing to say when there is no suggestion", () => {
    expect(suggestionRationale(deriveBackfillSuggestion([], NOW))).toBeNull();
  });
});
