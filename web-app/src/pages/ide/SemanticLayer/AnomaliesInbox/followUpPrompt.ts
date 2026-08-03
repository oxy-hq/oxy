/** The context block handed to the agent when the user asks a follow-up
 *  question about an anomaly.
 *
 *  Its own module because it is the surface where being wrong is least visible:
 *  every optional field here is interpolated into prose the model reads and the
 *  user never sees, so an unguarded `undefined` misinforms the answer silently.
 *  Pure and React-free so it can be pinned by tests. */
import { splitLabel } from "@/pages/ide/MetricTree/components/ExplainTree";
import type { MetricAnomaly } from "@/types/metricAnomalies";
import type { ExplainNode, ExplainResult, ExplainWarning } from "@/types/metricTree";
import { formatNumber, formatPercent, formatSigned } from "@/utils/measureFormat";
import { roleInstruction } from "./driverClassification";

export interface DerivedPeriods {
  current: [string, string];
  previous: [string, string];
}

/** Build the chat prompt: a fenced "context" block with the full decomposition
 *  (anomaly summary, period-over-period, recursive split tree, driver
 *  attributions, warnings) followed by the user's literal question. The agent
 *  sees the context as background; the question itself is what it answers.
 *
 *  We dump quite a bit so the agent can answer "which child segment drove the
 *  parent component?" without re-running the decomposition. */
export function buildFollowUpPrompt(
  anomaly: MetricAnomaly,
  periods: DerivedPeriods | null,
  result: ExplainResult | null,
  userQuestion: string
): string {
  const ctx: string[] = [
    `Anomaly: ${anomaly.label || anomaly.measure} (${anomaly.measure})`,
    `Bucket: ${anomaly.period_start.slice(0, 10)} (${anomaly.granularity})`,
    `Observed ${formatNumber(anomaly.observed)} vs expected baseline ${formatNumber(anomaly.expected)} (${anomaly.severity} severity, z=${anomaly.z_score.toFixed(2)})`
  ];
  if (periods) {
    ctx.push(`Period-over-period: current=${periods.current[0]}, previous=${periods.previous[0]}`);
  }
  if (result) {
    ctx.push(
      `Target moved ${formatNumber(result.target_previous)} → ${formatNumber(result.target_current)} (${formatSigned(result.target_delta)}); ${(result.coverage * 100).toFixed(0)}% of the delta is explained by the decomposition below.`
    );
    appendTreeLines(ctx, result);
    appendDriverLines(ctx, result);
    appendWarningLines(ctx, result);
  }
  return ["```context", ...ctx, "```", "", userQuestion].join("\n");
}

function appendTreeLines(ctx: string[], result: ExplainResult): void {
  if (result.nodes.length === 0) return;
  ctx.push("");
  ctx.push("Decomposition tree (each line = one split; indent = nesting):");
  for (const node of result.nodes) {
    appendNodeLines(ctx, node, 0);
  }
}

function appendDriverLines(ctx: string[], result: ExplainResult): void {
  if (!result.driver_attribution || result.driver_attribution.length === 0) return;
  ctx.push("");
  ctx.push("Declared drivers (causal/correlative inputs from the metric tree):");
  // Spell out each driver's role. Without it the agent reads a driver that moved
  // *against* the anomaly as one of its causes — the same mistake the
  // undifferentiated UI list used to invite.
  for (const d of result.driver_attribution) {
    const impact = d.estimated_target_impact;
    const impactStr =
      impact !== undefined && impact !== null
        ? ` → est. target impact ${formatSigned(impact)}`
        : " (qualitative — no coefficient, so no magnitude)";
    // `direction` is optional for the same reason `contribution` is: a
    // pre-classification cached explain has neither. Build the fragment
    // conditionally — interpolating the field directly put the literal
    // "undefined relationship" in front of the model, and `roleInstruction`
    // already tells it the direction is undetermined on those rows.
    const directionStr =
      d.direction && d.direction !== "unknown" ? ` · ${d.direction} relationship` : "";
    const coefStr =
      d.coefficient !== undefined && d.coefficient !== null ? ` · coef ${d.coefficient}` : "";
    ctx.push(
      `  • ${d.driver_measure} [${roleInstruction(d)}]: Δ ${formatSigned(d.driver_delta)} (${formatNumber(d.driver_previous)} → ${formatNumber(d.driver_current)})${directionStr}${coefStr} · ${d.form}${impactStr}`
    );
    if (d.passthrough) {
      const p = d.passthrough;
      ctx.push(
        `    MECHANICAL — tracks ${p.base_measure}: ratio ${formatPercent(p.ratio_previous)} → ${formatPercent(p.ratio_current)}; of its Δ, ${formatSigned(p.base_driven_delta)} is forced by the base and only ${formatSigned(p.ratio_driven_delta)} is the ratio itself`
      );
    }
    if (d.description) {
      ctx.push(`    relationship note: ${d.description}`);
    }
  }
}

function appendWarningLines(ctx: string[], result: ExplainResult): void {
  if (!result.warnings || result.warnings.length === 0) return;
  ctx.push("");
  ctx.push("Detector warnings on this decomposition:");
  for (const w of result.warnings) {
    ctx.push(`  • ${warningMessage(w)}`);
  }
}

/** Recursively append one indented line per node + its siblings + its recursive
 *  children to the context buffer. Mirrors the visual Decomposition tree the
 *  user sees in the drawer so the agent works off the same structure. */
function appendNodeLines(ctx: string[], node: ExplainNode, depth: number): void {
  const indent = "  ".repeat(depth + 1);
  ctx.push(
    `${indent}• ${splitLabel(node.split)} (measure ${node.measure}) — Δ ${formatSigned(node.delta)} · ${(node.root_fraction * 100).toFixed(1)}% of root · concentration ${(node.concentration * 100).toFixed(0)}%`
  );
  // Surface siblings inline so the agent knows the next-best alternatives at the
  // same level without us having to recurse into them.
  if (node.siblings && node.siblings.length > 0) {
    const sibSummary = node.siblings
      .slice(0, 4)
      .map((s) => `${splitLabel(s.split)} (${(s.root_fraction * 100).toFixed(1)}%)`)
      .join("; ");
    ctx.push(`${indent}  also considered: ${sibSummary}`);
  }
  if (node.children) {
    for (const child of node.children) {
      appendNodeLines(ctx, child, depth + 1);
    }
  }
}

/** Render an [`ExplainWarning`] as a single human-readable sentence. */
export function warningMessage(w: ExplainWarning): string {
  switch (w.type) {
    case "simpsons_paradox":
      return `Simpson's paradox on ${w.dimension}: aggregate moved ${formatSigned(w.aggregate_delta)} but every segment moved the opposite way.`;
    case "opposing_offset":
      return `Opposing offsets: ${w.component_a} ${formatSigned(w.delta_a)} cancels with ${w.component_b} ${formatSigned(w.delta_b)} — the net move hides a bigger shift in both components.`;
    case "non_additive_dimension_split":
      return `${w.measure} is a ${w.measure_type} measure — per-element deltas on ${w.dimension} don't sum to the parent delta, so concentrations are approximations.`;
  }
}
