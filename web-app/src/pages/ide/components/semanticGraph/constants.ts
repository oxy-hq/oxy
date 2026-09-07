/**
 * Shared geometry for the IDE's semantic-graph surfaces (World Model, Metric
 * Tree). Both surfaces draw the same kind of picture — a laid-out graph of
 * semantic-layer objects with a detail panel beside it — so the numbers that
 * decide how that picture reads live here once rather than per surface.
 */

/** Card width used by every graph node, and by ELK when reserving its box. */
export const NODE_WIDTH = 184;

/** Reserved height of a card in its default (unexpanded) state. */
export const NODE_HEIGHT_COLLAPSED = 80;

/** Tailwind width class for the detail panel beside a graph. */
export const PANEL_WIDTH = "w-96";
