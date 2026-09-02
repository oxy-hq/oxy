// Example: world-model + metric-tree analysis hooks in a customer-app bundle.
//
// Unlike `metric-tree.ts` (the API-key `OxyClient` surface, runnable as a
// standalone `pnpm tsx` script), these are the v2 React hooks a published
// customer app uses. They run inside `<OxyAppProvider>`, which resolves the
// app's identity from `oxy-app.json` and provides a session-cookie-
// credentialed fetcher — so there is no API key in the bundle.
//
// ── How to run ──────────────────────────────────────────────────────────────
// This file is illustrative component code, not a standalone app — it needs a
// customer-app dev harness (Vite + `@oxy-hq/vite-plugin`, which proxies `/api`
// to a running Oxy backend). To see it live:
//
//   1. Have Oxy running locally with a project whose semantic model declares
//      the views/measures you reference:  `oxy serve` (default :3000).
//   2. Scaffold a bundle harness:  `pnpm dlx create-oxy-app my-app`  (the
//      `vite` template is pre-wired with the plugin + an `oxy-app.json`).
//   3. Copy this file's components into the scaffold's `src/App.tsx`.
//   4. `pnpm install && pnpm dev`  → http://localhost:5173 (proxies /api → :3000).
//      For cross-origin dev, set `OXY_PROJECT=<uuid>` and `OXY_TOKEN` (see the
//      template README / `internal-docs/customer-apps.md` §3).
//
// NOTE: the measure / entity / view ids below (`orders.net_revenue`,
// `financials.operating_profit`, `store` / `net_revenue`, …) are placeholders —
// swap them for ids that exist in your project's semantic model.
//
// This single file wires five surfaces:
//   1. useWorldModelGraph   — the semantic-model entity/measure graph
//   2. useOpportunity       — segment opportunity sizing (addressable upside)
//   3. useExplain           — period-over-period root-cause decomposition (RCA)
//   4. useMeasureBreakdown  — per-instance driver-tree (SSE, fills in live)
//   5. useWorldModel        — the node-paradigm interface (expand / explain / …)

import type { ExpandedNode, ExplainResult } from "@oxy-hq/sdk";
import {
  OxyAppProvider,
  useExplain,
  useMeasureBreakdown,
  useOpportunity,
  useWorldModel,
  useWorldModelGraph,
  useWorldModelInstances
} from "@oxy-hq/sdk";
import * as React from "react";

export default function App() {
  return (
    <OxyAppProvider fallback={<p>Loading…</p>}>
      <WorldModelGraph />
      <OpportunitySizing target='orders.net_revenue' timeDimension='orders.order_date' />
      <RootCause target='financials.operating_profit' timeDimension='financials.month' />
      <InstanceDrivers entity='store' measure='net_revenue' />
      <NodeExplorer root='orders.net_revenue' timeDimension='orders.order_date' />
    </OxyAppProvider>
  );
}

// 1. The world-model graph: entities (nodes) + how measures promote up the
//    entity hierarchy (edges).
function WorldModelGraph() {
  const { data, loading, error } = useWorldModelGraph();
  if (loading) return <p>Loading graph…</p>;
  if (error) return <p>Error: {error.message}</p>;
  if (!data) return null;
  return (
    <section>
      <h2>World model — {data.entities.length} entities</h2>
      <ul>
        {data.entities.map((e) => (
          <li key={e.id}>
            <strong>{e.label}</strong> — {e.own_measures.length} own, {e.induced_measures.length}{" "}
            promoted measures
          </li>
        ))}
      </ul>
    </section>
  );
}

// 2. Opportunity sizing: "where is the upside, and what would closing it add?"
//    The engine benchmarks each segment's per-unit rate and sizes the gap
//    against the segment's own volume.
function OpportunitySizing({ target, timeDimension }: { target: string; timeDimension: string }) {
  const { data, loading, error } = useOpportunity({
    target,
    time_dimension: timeDimension,
    period: ["2025-04-01", "2025-06-30"]
  });
  if (loading) return <p>Sizing…</p>;
  if (error) return <p>Error: {error.message}</p>;
  if (!data) return null;
  return (
    <section>
      <h2>Opportunities on {target}</h2>
      {data.dimensions.map((d) => (
        <p key={d.dimension}>
          <strong>{d.dimension}</strong>: +{d.total_upside.toLocaleString()} if every
          below-benchmark segment reached its peer rate
        </p>
      ))}
    </section>
  );
}

// 3. Root-cause analysis: decompose a period-over-period move into the
//    components and dimensions that drove it.
function RootCause({ target, timeDimension }: { target: string; timeDimension: string }) {
  const { data, loading, error } = useExplain({
    target,
    time_dimension: timeDimension,
    current_period: ["2025-09-01", "2025-09-30"],
    previous_period: ["2025-08-01", "2025-08-31"]
  });
  if (loading) return <p>Explaining…</p>;
  if (error) return <p>Error: {error.message}</p>;
  if (!data) return null;
  return (
    <section>
      <h2>Why {target} moved</h2>
      <p>
        Δ {data.target_delta.toLocaleString()} — {(data.coverage * 100).toFixed(0)}% explained
      </p>
      <ul>
        {data.nodes.slice(0, 5).map((n) => (
          <li key={`${n.measure}:${n.root_fraction}`}>
            {n.measure}: {n.delta.toLocaleString()} ({(n.root_fraction * 100).toFixed(0)}% of move)
          </li>
        ))}
      </ul>
    </section>
  );
}

// 4. Per-instance driver tree (SSE): pick the first instance of `entity`, then
//    stream the decomposition of `measure` — node values fill in as they
//    resolve.
function InstanceDrivers({ entity, measure }: { entity: string; measure: string }) {
  const instances = useWorldModelInstances(entity, { limit: 1 });
  const first = instances.data?.items[0]?.key ?? null;
  const { breakdown, loading, done } = useMeasureBreakdown(entity, first, first ? measure : null);
  if (instances.loading) return <p>Loading instance…</p>;
  if (!breakdown) return <p>{loading ? "Streaming breakdown…" : null}</p>;
  return (
    <section>
      <h2>
        {measure} breakdown {done ? "(complete)" : "(streaming…)"}
      </h2>
      <ul>
        {breakdown.nodes.map((n) => (
          <li key={n.id}>
            {n.label}: {n.value ?? "…"}
            {n.unvalued_reason ? ` (${n.unvalued_reason})` : ""}
          </li>
        ))}
      </ul>
    </section>
  );
}

// 5. The node paradigm: start at one top-line metric, expand it into its
//    drivers/components (child cards), and explain any node inline. Every
//    object is a live handle rather than a static number — click a driver and
//    recurse. `useWorldModel()` returns the whole interface, scoped to the
//    project; `world.metric(id)` is the node you carry around.
function NodeExplorer({ root, timeDimension }: { root: string; timeDimension: string }) {
  const world = useWorldModel();
  const [children, setChildren] = React.useState<ExpandedNode[] | null>(null);
  const [rca, setRca] = React.useState<ExplainResult | null>(null);

  React.useEffect(() => {
    const ctrl = new AbortController();
    world
      .metric(root)
      .expand(ctrl.signal)
      .then(setChildren)
      .catch(() => setChildren([]));
    return () => ctrl.abort();
  }, [world, root]);

  const explainChild = (id: string) => {
    world
      .metric(id)
      .explain({
        time_dimension: timeDimension,
        current_period: ["2025-09-01", "2025-09-30"],
        previous_period: ["2025-08-01", "2025-08-31"]
      })
      .then(setRca)
      .catch(() => setRca(null));
  };

  return (
    <section>
      <h2>Walk the model from {root}</h2>
      <ul>
        {(children ?? []).map((c) => (
          <li key={c.node.id}>
            <button type='button' onClick={() => explainChild(c.node.id)}>
              {c.node.label}
            </button>{" "}
            <em>({c.edge.kind})</em>
          </li>
        ))}
      </ul>
      {rca ? (
        <p>
          Δ {rca.target_delta.toLocaleString()} on {rca.target} — {(rca.coverage * 100).toFixed(0)}%
          explained
        </p>
      ) : null}
    </section>
  );
}
