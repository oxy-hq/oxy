/**
 * Presentation helpers for a resource's airway `SourceContract`.
 *
 * Pure — no React — so the formatting rules are unit-testable the way
 * `airwayReducer` is. The component that uses them is `ContractBadge`.
 */

import type { ContractMutability, ResourceContract } from "@/services/api/airway";

/** Largest-first, so the formatter picks the coarsest unit that fits. */
const UNITS: ReadonlyArray<readonly [suffix: string, ms: number]> = [
  ["d", 86_400_000],
  ["h", 3_600_000],
  ["m", 60_000],
  ["s", 1_000],
  ["ms", 1]
];

/**
 * Milliseconds → a duration an operator can read at a glance: `7d`,
 * `30m`, `1h 30m`, `500ms`.
 *
 * At most the two coarsest non-zero units — a restatement window is a
 * policy knob, not a stopwatch, and `604800000` or `604800s` tells nobody
 * that it means a week. Never lossy in the direction that matters: a
 * sub-second value keeps its `ms` rather than rounding to `0s`.
 */
export function formatDurationMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms === 0) return "0s";
  let rest = Math.round(ms);
  const parts: string[] = [];
  for (const [suffix, size] of UNITS) {
    if (rest < size) continue;
    const n = Math.floor(rest / size);
    rest -= n * size;
    parts.push(`${n}${suffix}`);
    if (parts.length === 2) break;
  }
  return parts.join(" ");
}

export const MUTABILITY_LABEL: Record<ContractMutability, string> = {
  immutable: "immutable",
  versioned: "versioned",
  opaque: "opaque",
  undeclared: "undeclared"
};

/**
 * Badge styling per state — semantic tokens only.
 *
 * `versioned` gets the brand token because it is the state with the most
 * machinery behind it (a version guard on the write path), not because it
 * is "good". `undeclared` is dashed rather than coloured: it is a gap in
 * the connector's description, not a failure, and colouring it like one
 * would push operators to "fix" connectors that are behaving correctly.
 */
export const MUTABILITY_CLASS: Record<ContractMutability, string> = {
  immutable: "border-border bg-muted text-foreground",
  versioned: "border-primary/40 bg-primary/10 text-primary",
  opaque: "border-border bg-transparent text-muted-foreground",
  undeclared: "border-muted-foreground/40 border-dashed bg-transparent text-muted-foreground"
};

/**
 * The one fact worth showing beside the badge without opening the
 * tooltip: how far back of the cursor each pull reaches. Omitted when
 * zero (nothing to say) or unknown (nothing may be said).
 */
export function rewindHint(contract: ResourceContract): string | null {
  const rewind = contract.rewind_ms;
  if (rewind == null || rewind === 0) return null;
  return `−${formatDurationMs(rewind)}`;
}

function headline(contract: ResourceContract): string {
  switch (contract.mutability) {
    case "immutable":
      return `${contract.resource}: immutable — rows are never corrected once published.`;
    case "versioned":
      return `${contract.resource}: versioned on \`${contract.version_field ?? "?"}\` — corrections carry a version the write path can compare on.`;
    case "opaque":
      return `${contract.resource}: opaque — the source exposes no version, so a change is only detectable by re-reading the rows.`;
    case "undeclared":
      return `${contract.resource}: undeclared — this connector has not described how the resource behaves. Airway falls back to opaque handling, but that is a default, not a vendor fact.`;
  }
}

/**
 * Full detail as tooltip text — everything the grid deliberately doesn't
 * spend a column on.
 *
 * A field that is `null` on a *declared* contract is a real statement
 * ("declares no restatement window") and is rendered as such; on an
 * undeclared one there is nothing to render, which is why `headline`
 * carries the whole message there.
 */
export function contractTooltip(contract: ResourceContract): string {
  const lines = [headline(contract)];
  if (contract.mutability === "undeclared") return lines.join("\n");

  if (contract.version_column) {
    lines.push(`Landed version column: ${contract.version_column}`);
  }
  if (contract.cursor_tracks_modification != null) {
    lines.push(
      contract.cursor_tracks_modification
        ? "Cursor moves when a row is corrected, so a window can catch late edits."
        : "Cursor does NOT move on correction — a cursor window cannot see late edits."
    );
  }
  lines.push(
    contract.restatement_window_ms == null
      ? "Restatement window: none declared."
      : `Restatement window: ${formatDurationMs(contract.restatement_window_ms)}.`
  );
  if (contract.cursor_lag_ms != null) {
    lines.push(`Cursor lag: ${formatDurationMs(contract.cursor_lag_ms)}.`);
  }
  if (contract.rewind_ms != null) {
    lines.push(`Each pull rewinds ${formatDurationMs(contract.rewind_ms)} behind the cursor.`);
  }
  if (contract.requires_partition_repull) {
    lines.push("Corrections are caught by re-pulling whole partitions, not by a cursor window.");
  }
  return lines.join("\n");
}
