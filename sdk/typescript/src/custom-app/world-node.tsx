// The World Model **node interface** — the higher-level "node paradigm" from
// `docs/build/sdk/world-model.mdx`. Everything in the World Model is a node,
// and every node speaks the same verbs: `expand` (one hop of relationships),
// `drill` (narrow to a segment), `explain` (period-over-period root cause),
// and `size` (peer-gap opportunity). Render a node, let the user pick a verb,
// get more nodes back, recurse.
//
// This is a thin composition layer over the metric-tree analyses that already
// ship (`metric-tree-hooks.tsx` / the `MetricTreeClient`): the verbs map onto
// the same `/semantic/metric-tree*` endpoints, so a bundle typed against a
// handle matches what the server serializes verbatim. It adds no new backend.
//
// The logic lives in a framework-agnostic `createWorldModel(projectId,
// fetcher)` factory; `useWorldModel()` is a thin `useMemo` over it, scoped to
// the active `<OxyAppProvider>` project.
//
// **Alpha.** Per the doc, the node paradigm is a design preview and may
// change. `drill` is the one verb the backend cannot yet honor — the
// metric-tree endpoints take no segment/instance filter (the opportunity
// endpoint explicitly refuses it), and structural verbs are scope-invariant.
// So `drill` returns a scoped handle for interface fidelity, but the value
// verbs (`explain`/`size`) on a drilled handle throw
// {@link WorldModelScopeUnsupportedError} rather than silently returning
// population numbers for a scoped question.

import * as React from "react";
import type {
  ExplainRequest,
  ExplainResult,
  MetricEdge,
  MetricNode,
  MetricTree,
  OpportunityRequest,
  OpportunityResult,
  SensitivityResult
} from "../metricTree";
import { getJson, metricTreePath, postJson } from "./metric-tree-fetch";
import type { AppFetcher } from "./react";
import { useOxyApp } from "./react";

// ── Types ─────────────────────────────────────────────────────────────────────

/** A `dimension → value` scope narrowed onto a node via {@link MetricHandle.drill}. */
export type MetricScope = Readonly<Record<string, string>>;

/** Options for {@link MetricHandle.explain} — an {@link ExplainRequest} minus
 *  the `target`, which the handle supplies from its own id. */
export type ExplainOpts = Omit<ExplainRequest, "target">;

/** Options for {@link MetricHandle.size} — an {@link OpportunityRequest} minus
 *  the `target`. */
export type SizeOpts = Omit<OpportunityRequest, "target">;

/** One child revealed by {@link MetricHandle.expand}: the child measure's
 *  node, the edge that connects it to the parent, and a handle to recurse. */
export interface ExpandedNode {
  /** The child measure (a component or a driver of the parent). */
  node: MetricNode;
  /** The parent → child edge — `kind`, `direction`, `strength`, `form`, … */
  edge: MetricEdge;
  /** A live handle on the child, carrying the parent's scope. */
  handle: MetricHandle;
}

/**
 * A live handle on one metric node. Carry it around and call a verb; every
 * verb returns either more nodes (`expand`), a scoped handle (`drill`), or an
 * analysis result (`explain` / `size` / `drivers`).
 */
export interface MetricHandle {
  /** Fully-qualified measure id (`view.measure`). */
  readonly id: string;
  /** The scope narrowed onto this handle by `drill` (empty for a root handle). */
  readonly scope: MetricScope;
  /** The measure's own tree node (label, expr, is_composite). */
  node(signal?: AbortSignal): Promise<MetricNode>;
  /** One hop of relationships — the metric's components and drivers as child nodes. */
  expand(signal?: AbortSignal): Promise<ExpandedNode[]>;
  /** The declared drivers of this measure, ranked by influence (sensitivity). */
  drivers(signal?: AbortSignal): Promise<SensitivityResult>;
  /** Root-cause a period-over-period move: why it dropped or climbed. */
  explain(opts: ExplainOpts, signal?: AbortSignal): Promise<ExplainResult>;
  /** Compare this node to its peers across each dimension and size the gap. */
  size(opts: SizeOpts, signal?: AbortSignal): Promise<OpportunityResult>;
  /** Narrow into a segment or entity instance — returns a scoped handle. */
  drill(scope: Record<string, string>): MetricHandle;
}

/**
 * The World Model interface, scoped to one project. The whole surface hangs
 * off this: grab a {@link MetricHandle} with `metric(id)` and the handle
 * speaks the verbs, or pull the whole graph with `tree(root?)`.
 */
export interface WorldModelApi {
  /** The active project id, or `null` before `<OxyAppProvider>` resolves one. */
  readonly projectId: string | null;
  /** The metric tree, rooted anywhere you like (default: the whole tree). */
  tree(root?: string, signal?: AbortSignal): Promise<MetricTree>;
  /** A live handle on one measure node. */
  metric(id: string): MetricHandle;
}

/**
 * Thrown by the value verbs (`explain` / `size`) when called on a handle that
 * has been `drill`ed. The metric-tree backend cannot yet scope these analyses
 * to a segment, so failing loud beats returning population numbers for a
 * question that asked about one segment.
 */
export class WorldModelScopeUnsupportedError extends Error {
  readonly code = "world_model_scope_unsupported";
  readonly scope: MetricScope;
  constructor(verb: string, scope: MetricScope) {
    super(
      `${verb} on a drilled (scoped) node is not yet supported by the backend ` +
        `(scope: ${JSON.stringify(scope)}). Call ${verb} on the un-drilled node ` +
        `for population-level analysis.`
    );
    this.name = "WorldModelScopeUnsupportedError";
    this.scope = scope;
  }
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Build a {@link WorldModelApi} over a project id and fetcher. Framework-
 * agnostic — `useWorldModel()` wraps this for React, but it is directly
 * unit-testable with a mock fetcher.
 */
export function createWorldModel(projectId: string | null, fetcher: AppFetcher): WorldModelApi {
  const base = (): string => {
    if (!projectId) {
      throw new Error(
        "World Model unavailable: no active project (are you inside <OxyAppProvider>?)"
      );
    }
    return metricTreePath(projectId);
  };

  const tree = (root?: string, signal?: AbortSignal): Promise<MetricTree> => {
    const qs = root ? `?root=${encodeURIComponent(root)}` : "";
    return getJson<MetricTree>(fetcher, `${base()}${qs}`, signal);
  };

  const makeHandle = (id: string, scope: MetricScope): MetricHandle => {
    const scoped = Object.keys(scope).length > 0;
    return {
      id,
      scope,
      async node(signal) {
        const t = await tree(id, signal);
        const found = t.nodes.find((n) => n.id === id);
        if (!found) throw new Error(`measure '${id}' not found in the metric tree`);
        return found;
      },
      async expand(signal) {
        const t = await tree(id, signal);
        const byId = new Map(t.nodes.map((n) => [n.id, n] as const));
        // `from` is the parent, `to` the child (component/driver) — the same
        // orientation the IDE metric-tree graph lays out top-down.
        const children: ExpandedNode[] = [];
        for (const edge of t.edges) {
          if (edge.from !== id) continue;
          const childNode = byId.get(edge.to);
          if (!childNode) continue;
          children.push({ node: childNode, edge, handle: makeHandle(edge.to, scope) });
        }
        return children;
      },
      drivers(signal) {
        return getJson<SensitivityResult>(
          fetcher,
          `${base()}/${encodeURIComponent(id)}/sensitivity`,
          signal
        );
      },
      explain(opts, signal) {
        if (scoped) throw new WorldModelScopeUnsupportedError("explain", scope);
        return postJson<ExplainResult>(
          fetcher,
          `${base()}/explain`,
          { target: id, ...opts },
          signal
        );
      },
      size(opts, signal) {
        if (scoped) throw new WorldModelScopeUnsupportedError("size", scope);
        return postJson<OpportunityResult>(
          fetcher,
          `${base()}/opportunity`,
          { target: id, ...opts },
          signal
        );
      },
      drill(next) {
        return makeHandle(id, { ...scope, ...next });
      }
    };
  };

  return {
    projectId,
    tree,
    metric: (id: string) => makeHandle(id, {})
  };
}

// ── Hook ──────────────────────────────────────────────────────────────────────

/**
 * The World Model node interface, scoped to the active `<OxyAppProvider>`
 * project. Returns a stable {@link WorldModelApi} — grab a node with
 * `world.metric(id)` and let it speak the verbs.
 *
 * @example
 * ```tsx
 * const world = useWorldModel();
 * const revenue = world.metric("orders.net_revenue");
 * const children = await revenue.expand();       // components + drivers
 * const rca = await revenue.explain({
 *   time_dimension: "orders.order_date",
 *   current_period: ["2026-06-01", "2026-06-30"],
 *   previous_period: ["2026-05-01", "2026-05-31"],
 * });
 * ```
 *
 * @remarks
 * This is the node-paradigm hook. For the raw semantic-layer entity/measure
 * graph, use {@link useWorldModelGraph} instead.
 */
export function useWorldModel(): WorldModelApi {
  const { projectId, fetcher } = useOxyApp();
  return React.useMemo(() => createWorldModel(projectId ?? null, fetcher), [projectId, fetcher]);
}
