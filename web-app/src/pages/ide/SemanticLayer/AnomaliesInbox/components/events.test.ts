import { describe, expect, it } from "vitest";
import type { MetricAnomaly } from "@/types/metricAnomalies";
import { canMoveTo, groupIntoEvents, targetOf } from "./events";

function anomaly(over: Partial<MetricAnomaly> & Pick<MetricAnomaly, "id">): MetricAnomaly {
  return {
    workspace_id: "ws",
    measure: "sales",
    time_dimension: "order_date",
    granularity: "day",
    period_start: "2026-01-01",
    period_end: "2026-01-02",
    observed: 10,
    expected: 20,
    lower_bound: 15,
    upper_bound: 25,
    z_score: -2,
    severity: "low",
    status: "new",
    label: null,
    dimension_key: "",
    filters: null,
    event_id: null,
    detected_at: "2026-01-02T00:00:00Z",
    updated_at: "2026-01-02T00:00:00Z",
    ...over
  } as MetricAnomaly;
}

describe("groupIntoEvents", () => {
  it("collapses buckets sharing an event_id into one event, oldest bucket first", () => {
    const events = groupIntoEvents([
      anomaly({ id: "b", event_id: "e1", period_start: "2026-01-03", z_score: -1 }),
      anomaly({ id: "a", event_id: "e1", period_start: "2026-01-01", z_score: -4 })
    ]);
    expect(events).toHaveLength(1);
    expect(events[0].key).toBe("e1");
    expect(events[0].buckets.map((b) => b.id)).toEqual(["a", "b"]);
  });

  it("keys ungrouped rows individually so pre-event rows don't merge", () => {
    const events = groupIntoEvents([anomaly({ id: "a" }), anomaly({ id: "b" })]);
    // Selection is keyed off `key`; a shared null event_id must not collapse
    // two unrelated anomalies into one selectable row.
    expect(events.map((e) => e.key)).toEqual(["ungrouped:a", "ungrouped:b"]);
  });

  it("picks the peak by |z-score|, not recency", () => {
    const events = groupIntoEvents([
      anomaly({ id: "mild", event_id: "e1", period_start: "2026-01-02", z_score: -1.5 }),
      anomaly({ id: "worst", event_id: "e1", period_start: "2026-01-01", z_score: 6 })
    ]);
    expect(events[0].peak.id).toBe("worst");
  });

  it("rolls severity up to the event's worst bucket", () => {
    // A sustained slide files its later days `low`; reading severity off the
    // peak bucket alone would badge the whole event `low`.
    const events = groupIntoEvents([
      anomaly({ id: "a", event_id: "e1", severity: "high", z_score: -2 }),
      anomaly({ id: "b", event_id: "e1", severity: "low", period_start: "2026-01-02", z_score: -9 })
    ]);
    expect(events[0].peak.id).toBe("b");
    expect(events[0].severity).toBe("high");
  });
});

describe("targetOf", () => {
  it("names an event by its id, never by the buckets it happens to hold", () => {
    // A list response caps buckets per event, so sending the ids we received
    // would leave the rest of a long chain `new` while the toast reported
    // success.
    const events = groupIntoEvents([
      anomaly({ id: "b1", event_id: "e1" }),
      anomaly({ id: "b2", event_id: "e1", period_start: "2026-01-02" })
    ]);
    expect(targetOf(events, "new")).toEqual({
      onlyStatuses: ["new", "acknowledged"],
      ids: [],
      eventIds: ["e1"]
    });
  });

  it("falls back to row ids for pre-event rows, which have no event to name", () => {
    const events = groupIntoEvents([anomaly({ id: "a" }), anomaly({ id: "b" })]);
    expect(targetOf(events, "new")?.ids).toEqual(["a", "b"]);
  });

  it("spares dismissed buckets everywhere but the Dismissed tab", () => {
    const events = groupIntoEvents([anomaly({ id: "b1", event_id: "e1" })]);
    expect(targetOf(events, "new")?.onlyStatuses).not.toContain("dismissed");
    expect(targetOf(events, "all")?.onlyStatuses).not.toContain("dismissed");
    expect(targetOf(events, "dismissed")?.onlyStatuses).toContain("dismissed");
  });

  it("keeps live buckets together, so a chained one can't be stranded", () => {
    // A scan chains a fresh `new` bucket onto an acknowledged event; bounding
    // the write to just the row's own status would leave it in the New tab
    // under a toast saying the anomaly was handled.
    const events = groupIntoEvents([anomaly({ id: "b1", event_id: "e1", status: "acknowledged" })]);
    expect(targetOf(events, "acknowledged")?.onlyStatuses).toEqual(["new", "acknowledged"]);
    expect(targetOf([], "acknowledged")).toBeNull();
  });
});

describe("canMoveTo", () => {
  it("asks every bucket, not just the peak", () => {
    // The All tab returns an event's buckets whatever state they are in, and
    // the peak is picked by |z|. An event holding one `new` bucket under an
    // `acknowledged` peak must still offer Ack — the New badge counts it.
    const [event] = groupIntoEvents([
      anomaly({ id: "peak", event_id: "e1", status: "acknowledged", z_score: -9 }),
      anomaly({ id: "fresh", event_id: "e1", status: "new", period_start: "2026-01-02" })
    ]);
    expect(event.peak.id).toBe("peak");
    expect(canMoveTo(event, "acknowledged", "all")).toBe(true);
  });

  it("is false when the only differing bucket is out of scope", () => {
    // Outside the Dismissed tab a dismissed bucket cannot be written, so
    // offering Ack would send a write matching nothing and report the rows as
    // gone while they sat on screen.
    const [event] = groupIntoEvents([
      anomaly({ id: "peak", event_id: "e1", status: "acknowledged", z_score: -9 }),
      anomaly({ id: "gone", event_id: "e1", status: "dismissed", period_start: "2026-01-02" })
    ]);
    expect(canMoveTo(event, "acknowledged", "all")).toBe(false);
  });

  it("is false when every in-scope bucket is already there", () => {
    const [event] = groupIntoEvents([
      anomaly({ id: "a", event_id: "e1", status: "dismissed" }),
      anomaly({ id: "b", event_id: "e1", status: "dismissed", period_start: "2026-01-02" })
    ]);
    expect(canMoveTo(event, "dismissed", "dismissed")).toBe(false);
    // …and reachable from the tab where those buckets are what you see.
    expect(canMoveTo(event, "acknowledged", "dismissed")).toBe(true);
  });
});

describe("event status", () => {
  it("is the least-resolved bucket, not the peak's", () => {
    // The peak is chosen by |z|, so an All-tab event can show an
    // `acknowledged` peak while still holding a `new` bucket. Labelling that
    // row "acknowledged" while its Ack button is live means clicking it
    // changes nothing visible — the failure the label exists to prevent.
    const [event] = groupIntoEvents([
      anomaly({ id: "peak", event_id: "e1", status: "acknowledged", z_score: -9 }),
      anomaly({ id: "fresh", event_id: "e1", status: "new", period_start: "2026-01-02" })
    ]);
    expect(event.peak.status).toBe("acknowledged");
    expect(event.status).toBe("new");
  });

  it("falls through acknowledged to dismissed only when nothing is live", () => {
    const [acked] = groupIntoEvents([
      anomaly({ id: "a", event_id: "e1", status: "acknowledged" }),
      anomaly({ id: "b", event_id: "e1", status: "dismissed", period_start: "2026-01-02" })
    ]);
    expect(acked.status).toBe("acknowledged");

    const [gone] = groupIntoEvents([anomaly({ id: "c", event_id: "e2", status: "dismissed" })]);
    expect(gone.status).toBe("dismissed");
  });
});
