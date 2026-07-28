import type { WmSelection, WorldModel } from "@/types/worldModel";
import { formatCompact } from "./measureTarget";

/** Split a metric-tree node id (`view.measure`) into its view and measure name.
 *  The view is the first dotted segment, matching how the tree names nodes. */
function splitNodeId(nodeId: string): { view: string; measure: string } | null {
  const dot = nodeId.indexOf(".");
  if (dot <= 0 || dot === nodeId.length - 1) return null;
  return { view: nodeId.slice(0, dot), measure: nodeId.slice(dot + 1) };
}

/**
 * Resolve a metric-tree node id (`view.measure`) to the world-model selection
 * for that measure, so a driver row can navigate to the driver like any other
 * node. Prefers the entity that *declares* the measure (own measure); falls back
 * to an entity it is induced onto from that view. Returns `null` when the world
 * model doesn't host the measure — the caller renders it as inert text.
 */
export function measureNodeSelection(model: WorldModel, nodeId: string): WmSelection {
  const parts = splitNodeId(nodeId);
  if (!parts) return null;
  const { view, measure } = parts;

  const declaredOn = model.entities.find(
    (e) => e.view === view && e.own_measures.some((m) => m.name === measure)
  );
  if (declaredOn) {
    return { kind: "measure", entityId: declaredOn.id, measureName: measure, induced: false };
  }

  for (const e of model.entities) {
    const induced = e.induced_measures.find((m) => m.name === measure && m.promoted_from === view);
    if (induced) {
      return {
        kind: "measure",
        entityId: e.id,
        measureName: measure,
        induced: true,
        promotedFrom: view
      };
    }
  }
  return null;
}

/** Bare dimension name (`store_region`) from a possibly-qualified id (`order.store_region`). */
function bareDimension(dimension: string): string {
  const dot = dimension.indexOf(".");
  return dot < 0 ? dimension : dimension.slice(dot + 1);
}

/**
 * Resolve an opportunity dimension to its world-model dimension selection on the
 * entity that hosts `view`, so a dimension header can jump to the dimension node.
 * Returns `null` when it isn't a modelled dimension of that entity.
 */
export function dimensionNodeSelection(
  model: WorldModel,
  view: string,
  dimension: string
): WmSelection {
  const name = bareDimension(dimension);
  const host = model.entities.find(
    (e) => e.view === view && e.dimensions.some((d) => d.name === name)
  );
  return host ? { kind: "dimension", entityId: host.id, dimensionName: name } : null;
}

/** Short measure name from a `view.measure` node id, for prose. */
function measureLabel(nodeId: string): string {
  return splitNodeId(nodeId)?.measure ?? nodeId;
}

/**
 * A ready-to-run investigation question for a sized segment, so the panel hands
 * off a concrete prompt (copied to the clipboard — the Ask dock isn't mounted on
 * the IDE surface) rather than dead-ending on a number.
 */
/**
 * Tidy a segment value for display.
 *
 * The engine stringifies each segment straight off the JSON row, so a numeric
 * dimension arrives as a float literal and a segment of `3` reads as `"3.0"`.
 * Strip a trailing all-zero fraction so whole numbers read as whole numbers.
 *
 * Deliberately a regex on the digits rather than a `Number()` round-trip: this
 * only ever touches the exact `"<int>.<zeros>"` shape, so it can't reformat a
 * genuine decimal (`"1.5"`), mangle a large value into exponent form, or lose
 * precision on an id wider than a float. Non-numeric segments pass through
 * untouched, including ones that merely contain digits (`"v1.0"`).
 */
export function formatSegment(segment: string): string {
  return segment.replace(/^(-?\d+)\.0+$/, "$1");
}

/**
 * Build the investigation prompt for one segment row.
 *
 * `scope` must describe the same narrowing the panel applied, if any. The rates
 * and upside quoted here were measured inside that scope, so a prompt that
 * omits it asks the agent a population-wide question about instance-scoped
 * numbers — and the answer would contradict the panel it was copied from.
 */
export function segmentQuestion(args: {
  target: string;
  dimension: string;
  segment: string;
  currentRate: number;
  benchmark: number;
  upside: number;
  periodDays: number;
  scope?: { entity: string; key: string };
}): string {
  const measure = measureLabel(args.target);
  const dim = bareDimension(args.dimension);
  const within = args.scope ? ` within ${args.scope.entity} = "${args.scope.key}"` : "";
  return (
    `Over the last ${args.periodDays} days${within}, ${measure} for ${dim} = "${formatSegment(args.segment)}" ran at a ` +
    `per-unit rate of ${formatCompact(args.currentRate)} versus a benchmark of ` +
    `${formatCompact(args.benchmark)}, about ${formatCompact(args.upside)} of addressable upside. ` +
    `What is driving that gap, and how could we close it?`
  );
}
