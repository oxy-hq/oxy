// ── Analytics node model ───────────────────────────────────────────────────────
//
// Converts a flat list of SSE events emitted by an analytics run into an
// ordered list of AnalyticsNode values suitable for display in a DAG panel.
// This is a pure function with no browser or React dependencies.

import type { SseEvent } from "./useAnalyticsRun";

export type AnalyticsNodeStatus = "running" | "done" | "failed";
export type AnalyticsNodeKind = "step" | "domain";

export interface AnalyticsNode {
  /** Deterministic index-based id, unique within the event sequence. */
  id: string;
  kind: AnalyticsNodeKind;
  label: string;
  status: AnalyticsNodeStatus;
  /**
   * Nesting depth of this node.
   * - Step nodes: 0 = top-level step, 1 = sub-spec step inside a fan-out.
   * - Domain nodes: always 0 (depth is irrelevant for domain nodes).
   */
  depth: number;
  /** JSON payload shown on hover. Omitted for step nodes. */
  data?: Record<string, unknown>;
}

// ── Domain event → node label / data mapping ──────────────────────────────────

function domainNode(
  index: number,
  label: string,
  status: AnalyticsNodeStatus,
  data: Record<string, unknown>
): AnalyticsNode {
  return { id: `n-${index}`, kind: "domain", label, status, depth: 0, data };
}

// ── Builder ───────────────────────────────────────────────────────────────────

/**
 * Build an ordered list of {@link AnalyticsNode}s from a sequence of SSE events.
 *
 * Rules:
 * - `step_start` pushes a running step node at the current nesting depth, then
 *   increments depth (so inner steps from fan-out sub-specs get depth ≥ 1).
 * - `step_end` decrements depth then marks the most recently opened step at that
 *   depth as done/failed. This correctly terminates both sub-spec steps and the
 *   outer fan-out step.
 * - Known domain events push a domain node with their payload.
 * - `done`, `error`, and all other event types are ignored (no node).
 */
export function buildAnalyticsNodes(events: SseEvent[]): AnalyticsNode[] {
  const nodes: AnalyticsNode[] = [];
  let counter = 0;
  let depth = 0;

  const nextId = () => `n-${counter++}`;

  // Find the last running step at a specific nesting depth.
  const findLastRunningStepAt = (d: number) => {
    for (let i = nodes.length - 1; i >= 0; i--) {
      if (nodes[i].kind === "step" && nodes[i].status === "running" && nodes[i].depth === d)
        return i;
    }
    return -1;
  };

  for (const ev of events) {
    switch (ev.type) {
      case "step_start":
        nodes.push({
          id: nextId(),
          kind: "step",
          label: (ev.data.label as string) ?? "",
          status: "running",
          depth
        });
        depth++;
        break;

      case "step_end": {
        depth = Math.max(0, depth - 1);
        const idx = findLastRunningStepAt(depth);
        if (idx !== -1) {
          nodes[idx] = {
            ...nodes[idx],
            status: ev.data.outcome === "advanced" ? "done" : "failed"
          };
        }
        break;
      }

      case "schema_resolved":
        nodes.push(
          domainNode(counter++, "Schema Resolved", "done", {
            tables: ev.data.tables
          })
        );
        break;

      case "triage_completed":
        nodes.push(
          domainNode(counter++, "Triage", "done", {
            summary: ev.data.summary,
            question_type: ev.data.question_type,
            confidence: ev.data.confidence,
            ambiguities: ev.data.ambiguities
          })
        );
        break;

      case "intent_clarified": {
        const label = ev.data.selected_procedure ? `Procedure Selected` : "Intent Clarified";
        nodes.push(
          domainNode(counter++, label, "done", {
            question_type: ev.data.question_type,
            metrics: ev.data.metrics,
            dimensions: ev.data.dimensions,
            filters: ev.data.filters,
            ...(ev.data.selected_procedure && { selected_procedure: ev.data.selected_procedure })
          })
        );
        break;
      }

      case "spec_resolved":
        nodes.push(
          domainNode(counter++, "Spec Resolved", "done", {
            resolved_metrics: ev.data.resolved_metrics,
            resolved_tables: ev.data.resolved_tables,
            result_shape: ev.data.result_shape,
            assumptions: ev.data.assumptions,
            solution_source: ev.data.solution_source
          })
        );
        break;

      case "query_generated":
        nodes.push(domainNode(counter++, "SQL Generated", "done", { sql: ev.data.sql }));
        break;

      case "query_executed": {
        const success = ev.data.success as boolean;
        nodes.push(
          domainNode(
            counter++,
            success ? "Query Executed" : "Query Failed",
            success ? "done" : "failed",
            {
              query: ev.data.query,
              columns: ev.data.columns,
              rows: ev.data.rows,
              duration_ms: ev.data.duration_ms,
              row_count: ev.data.row_count,
              error: ev.data.error
            }
          )
        );
        break;
      }

      case "analytics_validation_failed":
        nodes.push(
          domainNode(counter++, "Validation Failed", "failed", {
            state: ev.data.state,
            reason: ev.data.reason,
            model_response: ev.data.model_response
          })
        );
        break;

      case "procedure_step_started":
        nodes.push(
          domainNode(counter++, `Procedure: ${ev.data.step}`, "running", {
            step: ev.data.step
          })
        );
        break;

      case "procedure_step_completed": {
        // Find the most recent running procedure node for this step and finalize it.
        const stepLabel = `Procedure: ${ev.data.step}`;
        let found = false;
        for (let i = nodes.length - 1; i >= 0; i--) {
          if (
            nodes[i].kind === "domain" &&
            nodes[i].label === stepLabel &&
            nodes[i].status === "running"
          ) {
            nodes[i] = {
              ...nodes[i],
              status: ev.data.success ? "done" : "failed",
              data: { ...nodes[i].data, error: ev.data.error }
            };
            found = true;
            break;
          }
        }
        if (!found) {
          // No matching started node — push a standalone completed node.
          nodes.push(
            domainNode(counter++, stepLabel, ev.data.success ? "done" : "failed", {
              step: ev.data.step,
              error: ev.data.error
            })
          );
        }
        break;
      }

      // Terminal / streaming events — no node
      default:
        break;
    }
  }

  return nodes;
}

// ── Step grouping ─────────────────────────────────────────────────────────────

export interface AnalyticsStepGroup {
  /** The step node, or `undefined` for domain nodes that precede the first step. */
  step?: AnalyticsNode;
  /** Domain nodes directly under this step (before any sub-spec starts). */
  children: AnalyticsNode[];
  /**
   * Nested step groups produced by fan-out sub-specs (depth ≥ 1).
   * Each sub-group has its own `step` and `children`.
   */
  subGroups: AnalyticsStepGroup[];
}

/**
 * Groups a flat {@link AnalyticsNode} list into a two-level hierarchy.
 *
 * - Top-level step nodes (depth 0) start a new {@link AnalyticsStepGroup}.
 * - Sub-spec step nodes (depth ≥ 1) start a sub-group inside the current
 *   top-level group (fan-out scenario).
 * - Domain nodes are attached to the current innermost group's `children`.
 * - Domain nodes that appear before any step form an "orphan" group where
 *   `step` is `undefined`.
 */
export function groupNodesByStep(nodes: AnalyticsNode[]): AnalyticsStepGroup[] {
  const groups: AnalyticsStepGroup[] = [];
  /** The most recent top-level group. */
  let currentTop: AnalyticsStepGroup | null = null;
  /** The most recent sub-spec group (fan-out child). Cleared when a new top-level step starts. */
  let currentSub: AnalyticsStepGroup | null = null;

  for (const node of nodes) {
    if (node.kind === "step") {
      if (node.depth === 0) {
        // Top-level step — start a new group
        currentTop = { step: node, children: [], subGroups: [] };
        currentSub = null;
        groups.push(currentTop);
      } else {
        // Sub-spec step (depth ≥ 1) — nested inside the current top-level group
        if (!currentTop) {
          // No parent yet; create an orphan top-level container
          currentTop = { step: undefined, children: [], subGroups: [] };
          groups.push(currentTop);
        }
        currentSub = { step: node, children: [], subGroups: [] };
        currentTop.subGroups.push(currentSub);
      }
    } else {
      // Domain node — attach to the current leaf group
      const leaf = currentSub ?? currentTop;
      if (!leaf) {
        // No group at all yet — create an orphan group
        const orphan: AnalyticsStepGroup = { step: undefined, children: [node], subGroups: [] };
        groups.push(orphan);
        currentTop = orphan;
      } else {
        leaf.children.push(node);
      }
    }
  }

  return groups;
}
