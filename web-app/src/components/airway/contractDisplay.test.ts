import { describe, expect, it } from "vitest";

import type { ResourceContract } from "@/services/api/airway";

import { contractTooltip, formatDurationMs, rewindHint } from "./contractDisplay";

const undeclared: ResourceContract = {
  resource: "orders",
  mutability: "undeclared",
  version_field: null,
  version_column: null,
  cursor_tracks_modification: null,
  restatement_window_ms: null,
  cursor_lag_ms: null,
  rewind_ms: null,
  requires_partition_repull: null
};

const versioned: ResourceContract = {
  resource: "orders",
  mutability: "versioned",
  version_field: "modifiedDate",
  version_column: "modified_date",
  cursor_tracks_modification: true,
  restatement_window_ms: 3 * 86_400_000,
  cursor_lag_ms: 30_000,
  rewind_ms: 3 * 86_400_000 + 30_000,
  requires_partition_repull: false
};

describe("formatDurationMs", () => {
  it("renders operator-scale windows in their coarsest unit", () => {
    expect(formatDurationMs(7 * 86_400_000)).toBe("7d");
    expect(formatDurationMs(30 * 60_000)).toBe("30m");
    expect(formatDurationMs(2 * 3_600_000)).toBe("2h");
    expect(formatDurationMs(45_000)).toBe("45s");
  });

  it("composes at most two units", () => {
    expect(formatDurationMs(90 * 60_000)).toBe("1h 30m");
    expect(formatDurationMs(3 * 86_400_000 + 30_000)).toBe("3d 30s");
  });

  it("keeps sub-second values instead of rounding them to zero", () => {
    // `0s` here would read as "declares no lag", which the contract never said.
    expect(formatDurationMs(500)).toBe("500ms");
    expect(formatDurationMs(0)).toBe("0s");
  });

  it("refuses to invent a value for a nonsensical input", () => {
    expect(formatDurationMs(Number.NaN)).toBe("—");
    expect(formatDurationMs(-1)).toBe("—");
  });
});

describe("rewindHint", () => {
  it("summarises the rewind beside the badge", () => {
    expect(rewindHint(versioned)).toBe("−3d 30s");
  });

  it("says nothing when the rewind is zero or unknown", () => {
    expect(rewindHint({ ...versioned, rewind_ms: 0 })).toBeNull();
    expect(rewindHint(undeclared)).toBeNull();
  });
});

describe("contractTooltip", () => {
  it("names the undeclared case as a gap, never as opaque", () => {
    const text = contractTooltip(undeclared);
    expect(text).toContain("undeclared");
    expect(text).toContain("not described");
    // It must not assert any of the things a declared contract states.
    expect(text).not.toContain("Restatement window");
    expect(text).not.toContain("Cursor lag");
  });

  it("spells out every declared knob", () => {
    const text = contractTooltip(versioned);
    expect(text).toContain("modifiedDate");
    expect(text).toContain("Landed version column: modified_date");
    expect(text).toContain("Restatement window: 3d.");
    expect(text).toContain("Cursor lag: 30s.");
    expect(text).toContain("rewinds 3d 30s");
  });

  it("distinguishes a declared absence from an unknown", () => {
    const noWindow: ResourceContract = {
      ...versioned,
      restatement_window_ms: null,
      rewind_ms: 30_000
    };
    expect(contractTooltip(noWindow)).toContain("Restatement window: none declared.");
  });

  it("warns when a cursor cannot see late edits", () => {
    const blind: ResourceContract = {
      ...versioned,
      cursor_tracks_modification: false,
      requires_partition_repull: true
    };
    const text = contractTooltip(blind);
    expect(text).toContain("does NOT move on correction");
    expect(text).toContain("re-pulling whole partitions");
  });
});
