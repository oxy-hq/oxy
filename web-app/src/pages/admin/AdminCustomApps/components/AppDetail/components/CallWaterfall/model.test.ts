import { describe, expect, it } from "vitest";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { InvocationCall } from "@/types/apps";
import {
  barGeometry,
  callBarClass,
  callLabel,
  callOutcome,
  callTitle,
  incompleteNote,
  MIN_BAR_PCT,
  timelineTotalMs,
  unrecordedText
} from "./model";

const call = (over: Partial<InvocationCall>): InvocationCall => ({
  seq: 0,
  op: "query",
  family: "query",
  start_ms: 0,
  duration_ms: 10,
  status: "ok",
  ...over
});

describe("callLabel", () => {
  it("says what each family did from its facets alone", () => {
    expect(callLabel(call({ verb: "SELECT", target: "orders", rows: 120 }))).toBe(
      "SELECT orders · 120 rows"
    );
    expect(callLabel(call({ verb: "SELECT", target: "orders", rows: 1 }))).toBe(
      "SELECT orders · 1 row"
    );
    expect(callLabel(call({ verb: "SELECT", target: "t", rows: 10, rows_truncated: true }))).toBe(
      "SELECT t · 10 rows (truncated)"
    );
    // A stream's reply may not carry the rows shape: the label goes without.
    expect(
      callLabel(call({ op: "queryStream", family: "query_stream", verb: "SELECT", target: "t" }))
    ).toBe("SELECT t");
    expect(
      callLabel(
        call({ op: "fetch", family: "fetch", verb: "POST", target: "api.x.com", http_status: 200 })
      )
    ).toBe("POST api.x.com · 200");
    expect(callLabel(call({ op: "fetch", family: "fetch", target: "api.x.com" }))).toBe(
      "GET api.x.com"
    );
    for (const family of ["oltp", "airhouse", "warehouse", "tx"]) {
      expect(
        callLabel(
          call({ op: `${family}.exec`, family, verb: "UPDATE", target: "app_store.orders" })
        )
      ).toBe("UPDATE app_store.orders");
    }
    expect(callLabel(call({ op: "oltp.exec", family: "oltp" }))).toBe("oltp.exec");
    expect(callLabel(call({ op: "email.send", family: "email", count: 3 }))).toBe("3 recipients");
    expect(callLabel(call({ op: "notify", family: "notify", count: 2 }))).toBe("2 targets");
    expect(callLabel(call({ op: "semantic.query", family: "semantic", count: 4 }))).toBe(
      "4 measures"
    );
    expect(callLabel(call({ op: "storage.put", family: "storage" }))).toBe("storage.put");
    expect(callLabel(call({ op: "airway.run", family: "airway", target: "toast" }))).toBe(
      "airway.run toast"
    );
    expect(callLabel(call({ op: "query", family: "query" }))).toBe("query");
  });
});

describe("callOutcome", () => {
  it("names a failure by its class, and a hold or refusal by the policy's word", () => {
    expect(callOutcome(call({ status: "error", error_type: "timeout" }))).toBe("✕ timeout");
    expect(callOutcome(call({ status: "error" }))).toBe("✕ error");
    expect(callOutcome(call({ status: "held" }))).toBe("held");
    expect(callOutcome(call({ status: "refused" }))).toBe("refused");
    expect(callOutcome(call({ status: "ok" }))).toBeNull();
  });

  it("puts op, label, outcome, duration and offset in the tooltip", () => {
    const title = callTitle(
      call({
        op: "fetch",
        family: "fetch",
        target: "h.io",
        status: "error",
        error_type: "timeout",
        start_ms: 40,
        duration_ms: 1500
      })
    );
    expect(title).toBe("fetch — GET h.io — ✕ timeout — 1.50 s, started at +40 ms");
  });
});

describe("callBarClass", () => {
  it("colours by status only, from ADMIN_TONE, and never warns for a hold", () => {
    expect(callBarClass("error")).toBe(ADMIN_TONE.danger.dot);
    expect(callBarClass("held")).toBe(ADMIN_TONE.muted.dot);
    expect(callBarClass("refused")).toBe(ADMIN_TONE.muted.dot);
    expect(callBarClass("ok")).not.toBe(ADMIN_TONE.danger.dot);
    for (const status of ["ok", "error", "held", "refused"] as const) {
      expect(callBarClass(status)).not.toBe(ADMIN_TONE.warn.dot);
    }
  });
});

describe("bar geometry", () => {
  it("places a bar at start / total with width duration / total", () => {
    expect(barGeometry(250, 500, 1000)).toEqual({ leftPct: 25, widthPct: 50 });
  });

  it("draws a 0 ms call at the minimum width", () => {
    expect(barGeometry(100, 0, 1000).widthPct).toBe(MIN_BAR_PCT);
  });

  it("pulls a bar that starts past the end back inside the track", () => {
    const { leftPct, widthPct } = barGeometry(1200, 100, 1000);
    expect(leftPct + widthPct).toBeLessThanOrEqual(100);
    expect(leftPct).toBeGreaterThanOrEqual(0);
  });

  it("stretches the axis to the last call's end and the isolate start", () => {
    expect(
      timelineTotalMs({
        duration_ms: 100,
        init_ms: 20,
        calls: [call({ start_ms: 90, duration_ms: 30 })]
      })
    ).toBe(120);
    expect(timelineTotalMs({ duration_ms: null, init_ms: null, calls: [] })).toBe(1);
  });
});

describe("trace notes", () => {
  it("says the cap kept the first calls to start, and how many had not returned", () => {
    expect(incompleteNote({ calls: [], total_calls: 0, dropped: 0, unfinished: 0 })).toBeNull();
    const kept = Array.from({ length: 197 }, (_, seq) => call({ seq }));
    expect(incompleteNote({ calls: kept, total_calls: 1140, dropped: 940, unfinished: 3 })).toBe(
      "Showing the first 200 calls to start of 1,140. 3 calls had not returned when the invocation ended."
    );
    expect(incompleteNote({ calls: [], total_calls: 1, dropped: 0, unfinished: 1 })).toBe(
      "1 call had not returned when the invocation ended."
    );
  });

  it("explains each missing trace by its reason", () => {
    expect(unrecordedText("expired", 3)).toContain("kept for 3 days");
    expect(unrecordedText("running", 3)).toContain("Still running");
    expect(unrecordedText("did_not_start", 3)).toContain("never reached the function's code");
    expect(unrecordedText("not_recorded", 3)).toContain("No call trace was recorded");
  });
});
