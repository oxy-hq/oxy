// Metric Tree demo — exercises the SDK's `client.metricTree` namespace.
// Fetches the tree, lets you click a measure to see its drivers, and
// runs an explain decomposition with a default period over the most
// recent two months.

import { useEffect, useMemo, useRef, useState } from "react";
import {
  type ExplainResult,
  type MetricEdge,
  type MetricNode,
  type MetricTree,
  type OpportunityResult,
  type SensitivityResult,
  useOxy,
} from "@oxy-hq/sdk";

type Status = "idle" | "loading" | "success" | "error";

function firstOfMonthOffset(monthsBack: number): string {
  const d = new Date();
  d.setUTCDate(1);
  d.setUTCMonth(d.getUTCMonth() - monthsBack);
  return d.toISOString().slice(0, 10);
}

function lastOfMonthOffset(monthsBack: number): string {
  const d = new Date();
  d.setUTCDate(1);
  d.setUTCMonth(d.getUTCMonth() - monthsBack + 1);
  d.setUTCDate(0);
  return d.toISOString().slice(0, 10);
}

function formatValue(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  const abs = Math.abs(n);
  if (abs >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  if (abs >= 1) return n.toFixed(2);
  return n.toFixed(4);
}

// ── Contribution chart ───────────────────────────────────────────────────────

interface ContributionChartProps {
  nodes: Array<{ measure: string; root_fraction: number; delta: number }>;
}

/** Horizontal stacked-bar chart: each top-level decomposition child's
 *  contribution to the root delta, proportional to |root_fraction|. */
function ContributionChart({ nodes }: ContributionChartProps) {
  // Sort by |root_fraction| descending; cap at 8 to keep the chart legible.
  const sorted = [...nodes]
    .sort((a, b) => Math.abs(b.root_fraction) - Math.abs(a.root_fraction))
    .slice(0, 8);

  const maxAbs = Math.max(...sorted.map((n) => Math.abs(n.root_fraction)), 0.0001);
  const palette = [
    "#3b82f6",
    "#10b981",
    "#f59e0b",
    "#ef4444",
    "#8b5cf6",
    "#ec4899",
    "#14b8a6",
    "#6366f1",
  ];

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {sorted.map((node, i) => {
        const widthPct = (Math.abs(node.root_fraction) / maxAbs) * 100;
        const isNegative = node.root_fraction < 0;
        return (
          <div key={`${node.measure}-${i}`} style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div
              style={{
                width: 220,
                fontSize: "0.8rem",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                fontFamily: "monospace",
                color: "#444",
              }}
              title={node.measure}
            >
              {node.measure}
            </div>
            <div style={{ flex: 1, height: 22, background: "#f3f4f6", borderRadius: 3, position: "relative" }}>
              <div
                style={{
                  width: `${widthPct}%`,
                  height: "100%",
                  background: isNegative ? "#ef4444" : palette[i % palette.length],
                  borderRadius: 3,
                  transition: "width 0.2s",
                }}
              />
            </div>
            <div style={{ width: 110, textAlign: "right", fontSize: "0.75rem", fontFamily: "monospace", color: "#555" }}>
              {(node.root_fraction * 100).toFixed(1)}% · Δ{formatValue(node.delta)}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Tree rendering ───────────────────────────────────────────────────────────

interface TreeNodeProps {
  node: MetricNode;
  depth: number;
  /** Edge kind that connects this node to its parent (null for roots). */
  edgeKind: "component" | "driver" | null;
  childrenOf: Map<string, MetricEdge[]>;
  nodeById: Map<string, MetricNode>;
  selectedId: string | null;
  onSelect: (node: MetricNode) => void;
  /** Guard against cycles — a node already on this branch is not re-recursed. */
  ancestors: Set<string>;
}

function TreeNode({
  node,
  depth,
  edgeKind,
  childrenOf,
  nodeById,
  selectedId,
  onSelect,
  ancestors,
}: TreeNodeProps) {
  const childEdges = childrenOf.get(node.id) ?? [];
  const isSelected = selectedId === node.id;

  // Each level indents 16px. The vertical line is rendered via border-left.
  const indent = depth * 16;
  const nextAncestors = useMemo(() => {
    const s = new Set(ancestors);
    s.add(node.id);
    return s;
  }, [ancestors, node.id]);

  return (
    <div style={{ paddingLeft: indent }}>
      <div
        onClick={() => onSelect(node)}
        style={{
          cursor: "pointer",
          padding: "0.4rem 0.6rem",
          borderRadius: 4,
          borderLeft: depth > 0 ? "2px solid #e0e0e0" : "none",
          background: isSelected ? "#eaf2ff" : "transparent",
          marginBottom: 2,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "0.4rem" }}>
          {edgeKind && (
            <span
              style={{
                fontSize: "0.65rem",
                padding: "1px 6px",
                borderRadius: 8,
                background: edgeKind === "component" ? "#dbe7ff" : "#ffe6d6",
                color: edgeKind === "component" ? "#1d4ed8" : "#9a3412",
                fontWeight: 600,
                letterSpacing: 0.4,
                textTransform: "uppercase",
              }}
            >
              {edgeKind}
            </span>
          )}
          <span style={{ fontWeight: 500 }}>{node.label}</span>
          {node.is_composite && (
            <span style={{ fontSize: "0.7rem", color: "#666" }}>· composite</span>
          )}
        </div>
        <div style={{ fontSize: "0.75rem", color: "#666" }}>
          {node.id} — {node.measure_type}
        </div>
      </div>

      {childEdges.map((edge) => {
        const childNode = nodeById.get(edge.from);
        if (!childNode || ancestors.has(edge.from)) return null;
        return (
          <TreeNode
            key={`${edge.from}->${edge.to}`}
            node={childNode}
            depth={depth + 1}
            edgeKind={edge.kind}
            childrenOf={childrenOf}
            nodeById={nodeById}
            selectedId={selectedId}
            onSelect={onSelect}
            ancestors={nextAncestors}
          />
        );
      })}
    </div>
  );
}

// Curated time-dimension defaults for the seeded example views. Real schemas
// vary; this just makes the demo work out of the box. The input field stays
// editable so the user can override for unfamiliar views.
const TIME_DIMENSION_DEFAULTS: Record<string, string> = {
  orders: "orders.order_date",
  order_items: "orders.order_date",
  order_returns: "order_returns.return_date",
  order_shipments: "order_shipments.shipment_date",
  marketing_funnel: "marketing_funnel.event_date",
  marketing_spend: "marketing_spend.event_date",
  employees: "employees.month",
  operating_costs: "operating_costs.cost_date",
  financials: "financials.month",
};

function defaultTimeDimension(measure: MetricNode): string {
  return TIME_DIMENSION_DEFAULTS[measure.view] ?? `${measure.view}.event_date`;
}

export default function MetricTreeView() {
  const { sdk } = useOxy();
  const [tree, setTree] = useState<MetricTree | null>(null);
  const [treeStatus, setTreeStatus] = useState<Status>("idle");
  const [selected, setSelected] = useState<MetricNode | null>(null);
  const detailRef = useRef<HTMLDivElement | null>(null);

  // Scroll the detail panel into view when a node gets selected — the
  // panel renders below the tree (and below the Apps section above), so
  // without this the click feels like nothing happened on long pages.
  useEffect(() => {
    if (selected && detailRef.current) {
      detailRef.current.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }, [selected]);
  const [sensitivity, setSensitivity] = useState<SensitivityResult | null>(null);
  const [explain, setExplain] = useState<ExplainResult | null>(null);
  const [opportunity, setOpportunity] = useState<OpportunityResult | null>(null);
  const [actionStatus, setActionStatus] = useState<Status>("idle");
  const [error, setError] = useState<string>("");
  const [timeDimension, setTimeDimension] = useState<string>("");

  // Build a parent → children adjacency from edges. Airlayer convention:
  // an edge `from → to` means `from` is a component / driver of `to`, so
  // `to` is the parent (composite) and `from` is the child (input).
  const { roots, childrenOf, nodeById } = useMemo(() => {
    const byId = new Map<string, MetricNode>();
    for (const n of tree?.nodes ?? []) byId.set(n.id, n);

    const children = new Map<string, MetricEdge[]>();
    const hasParent = new Set<string>();
    for (const e of tree?.edges ?? []) {
      // parent = e.to, child = e.from
      if (!children.has(e.to)) children.set(e.to, []);
      children.get(e.to)!.push(e);
      hasParent.add(e.from);
    }

    // Roots = nodes that don't feed into anything else (top of the tree).
    const rootNodes = (tree?.nodes ?? [])
      .filter((n) => !hasParent.has(n.id))
      .sort((a, b) => a.id.localeCompare(b.id));

    return { roots: rootNodes, childrenOf: children, nodeById: byId };
  }, [tree]);

  const loadTree = async () => {
    if (!sdk) return;
    setTreeStatus("loading");
    setError("");
    try {
      const t = await sdk.metricTree.getTree();
      setTree(t);
      setTreeStatus("success");
    } catch (e) {
      setError((e as Error).message);
      setTreeStatus("error");
    }
  };

  const handleSelect = async (node: MetricNode) => {
    if (!sdk) return;
    setSelected(node);
    setSensitivity(null);
    setExplain(null);
    setOpportunity(null);
    const td = defaultTimeDimension(node);
    setTimeDimension(td);
    setActionStatus("loading");
    setError("");
    try {
      // Run sensitivity (driver ranking) and explain (value + contribution)
      // in parallel — sensitivity always succeeds since it doesn't need a
      // time dimension; explain may fail when the heuristic time-dimension
      // guess is wrong, in which case we still show the drivers.
      const [sens, exp] = await Promise.allSettled([
        sdk.metricTree.getSensitivity(node.id),
        sdk.metricTree.explain({
          target: node.id,
          time_dimension: td,
          current_period: [firstOfMonthOffset(1), lastOfMonthOffset(1)],
          previous_period: [firstOfMonthOffset(2), lastOfMonthOffset(2)],
        }),
      ]);
      if (sens.status === "fulfilled") setSensitivity(sens.value);
      if (exp.status === "fulfilled") setExplain(exp.value);
      // Auto-explain runs with a guessed time dimension and may fail when
      // the guess is wrong. Don't block the panel — just skip the value
      // card and let the user re-run with the right time dim. Sensitivity
      // failures are real (no time-dim dependency) so we do surface those.
      if (sens.status === "rejected") {
        setError((sens.reason as Error).message);
      }
      setActionStatus("success");
    } catch (e) {
      setError((e as Error).message);
      setActionStatus("error");
    }
  };

  const runExplain = async () => {
    if (!sdk || !selected || !timeDimension) return;
    setActionStatus("loading");
    setError("");
    try {
      const result = await sdk.metricTree.explain({
        target: selected.id,
        time_dimension: timeDimension,
        current_period: [firstOfMonthOffset(1), lastOfMonthOffset(1)],
        previous_period: [firstOfMonthOffset(2), lastOfMonthOffset(2)],
      });
      setExplain(result);
      setOpportunity(null);
      setActionStatus("success");
    } catch (e) {
      setError((e as Error).message);
      setActionStatus("error");
    }
  };

  const runOpportunity = async () => {
    if (!sdk || !selected || !timeDimension) return;
    setActionStatus("loading");
    setError("");
    try {
      const result = await sdk.metricTree.findOpportunities({
        target: selected.id,
        time_dimension: timeDimension,
        period: [firstOfMonthOffset(1), lastOfMonthOffset(1)],
      });
      setOpportunity(result);
      setExplain(null);
      setActionStatus("success");
    } catch (e) {
      setError((e as Error).message);
      setActionStatus("error");
    }
  };

  return (
    <>
      <div className="section">
        <div className="section-header">
          <h2>Metric Tree</h2>
          <button onClick={loadTree} className="btn btn-primary" disabled={treeStatus === "loading"}>
            🌳 {tree ? "Refresh Tree" : "Load Tree"}
          </button>
        </div>

        {treeStatus === "loading" && (
          <div className="alert alert-loading">
            <div className="spinner"></div>
            Loading metric tree...
          </div>
        )}

        {tree && roots.length > 0 && (
          <div className="metric-tree">
            {roots.map((root) => (
              <TreeNode
                key={root.id}
                node={root}
                depth={0}
                edgeKind={null}
                childrenOf={childrenOf}
                nodeById={nodeById}
                selectedId={selected?.id ?? null}
                onSelect={handleSelect}
                ancestors={new Set()}
              />
            ))}
          </div>
        )}

        {tree && roots.length === 0 && treeStatus === "success" && (
          <div className="empty-state">
            <p>Tree loaded but the semantic model has no measures.</p>
          </div>
        )}

        {!tree && treeStatus === "idle" && (
          <div className="empty-state">
            <p>Click “Load Tree” to fetch the workspace's metric tree.</p>
          </div>
        )}
      </div>

      {selected && (
        <div ref={detailRef} className="section">
          <div className="section-header">
            <h2>{selected.label}</h2>
            <div style={{ display: "flex", gap: "0.5rem", alignItems: "center", flexWrap: "wrap" }}>
              <input
                type="text"
                value={timeDimension}
                onChange={(e) => setTimeDimension(e.target.value)}
                placeholder="time dimension (e.g. orders.order_date)"
                style={{
                  padding: "0.4rem 0.6rem",
                  border: "1px solid #ccc",
                  borderRadius: 4,
                  fontSize: "0.85rem",
                  minWidth: 260,
                }}
              />
              <button
                onClick={runExplain}
                className="btn btn-secondary"
                disabled={!timeDimension || actionStatus === "loading"}
              >
                🔍 Explain (last month vs prior)
              </button>
              <button
                onClick={runOpportunity}
                className="btn btn-secondary"
                disabled={!timeDimension || actionStatus === "loading"}
              >
                📈 Find opportunities
              </button>
            </div>
          </div>

          {error && (
            <div className="alert alert-error">
              <strong>Error:</strong> {error}
            </div>
          )}
          {actionStatus === "loading" && (
            <div className="alert alert-loading">
              <div className="spinner"></div>
              Running...
            </div>
          )}

          {actionStatus === "success" && !sensitivity && !explain && !opportunity && (
            <div className="empty-state">
              <p>
                No analyses ran for this measure. Sensitivity is unavailable when the measure has
                no declared drivers; explain / opportunity need a valid time dimension above.
              </p>
            </div>
          )}

          {sensitivity && (
            <div style={{ marginTop: "1rem" }}>
              <h3 style={{ fontSize: "1rem", marginBottom: "0.5rem" }}>
                Drivers ({sensitivity.drivers.length})
              </h3>
              {sensitivity.drivers.length === 0 ? (
                <div className="empty-state">
                  <p>No declared drivers for this measure.</p>
                </div>
              ) : (
                <div className="app-list">
                  {sensitivity.drivers.map((d) => (
                    <div key={`${d.measure}-${d.path.join("/")}`} className="app-item">
                      <div className="app-name">{d.measure}</div>
                      <div className="app-path">
                        {d.direction} · {d.strength} · {d.edge_kind}
                        {d.effective_coefficient != null && ` · coef ${d.effective_coefficient.toFixed(3)}`}
                        {d.form && ` · ${d.form}`}
                        {d.lag != null && ` · lag ${d.lag}d`}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {explain && (
            <div style={{ marginTop: "1rem" }}>
              {/* Prominent metric value + period-over-period delta */}
              <div
                style={{
                  display: "flex",
                  alignItems: "baseline",
                  gap: "1rem",
                  padding: "0.75rem 1rem",
                  background: "#f9fafb",
                  border: "1px solid #e5e7eb",
                  borderRadius: 6,
                  marginBottom: "1rem",
                }}
              >
                <div>
                  <div style={{ fontSize: "0.7rem", color: "#666", textTransform: "uppercase", letterSpacing: 0.5 }}>
                    Current
                  </div>
                  <div style={{ fontSize: "1.5rem", fontWeight: 600, fontFamily: "monospace" }}>
                    {formatValue(explain.target_current)}
                  </div>
                </div>
                <div>
                  <div style={{ fontSize: "0.7rem", color: "#666", textTransform: "uppercase", letterSpacing: 0.5 }}>
                    Δ vs prior
                  </div>
                  <div
                    style={{
                      fontSize: "1.1rem",
                      fontFamily: "monospace",
                      color: explain.target_delta >= 0 ? "#10b981" : "#ef4444",
                    }}
                  >
                    {explain.target_delta >= 0 ? "+" : ""}
                    {formatValue(explain.target_delta)}{" "}
                    <span style={{ fontSize: "0.85rem", color: "#888" }}>
                      ({explain.target_previous.toFixed(0)} →{" "}
                      {explain.target_current.toFixed(0)})
                    </span>
                  </div>
                </div>
                <div style={{ marginLeft: "auto", textAlign: "right" }}>
                  <div style={{ fontSize: "0.7rem", color: "#666", textTransform: "uppercase", letterSpacing: 0.5 }}>
                    Explained
                  </div>
                  <div style={{ fontSize: "1.1rem", fontFamily: "monospace" }}>
                    {(explain.coverage * 100).toFixed(0)}%
                  </div>
                </div>
              </div>

              <h3 style={{ fontSize: "1rem", marginBottom: "0.5rem" }}>Contribution</h3>
              {explain.nodes.length === 0 ? (
                <div className="empty-state">
                  <p>No decomposition found.</p>
                </div>
              ) : (
                <ContributionChart nodes={explain.nodes} />
              )}
            </div>
          )}

          {opportunity && (
            <div style={{ marginTop: "1rem" }}>
              <h3 style={{ fontSize: "1rem", marginBottom: "0.5rem" }}>
                Opportunities ({opportunity.dimensions.length})
              </h3>
              <p style={{ fontSize: "0.85rem", color: "#555" }}>
                Overall {opportunity.overall_value.toFixed(2)} · weights = {opportunity.weight_basis}
              </p>
              {opportunity.dimensions.length === 0 ? (
                <div className="empty-state">
                  <p>No actionable opportunities.</p>
                </div>
              ) : (
                <div className="app-list">
                  {opportunity.dimensions.map((dim) => (
                    <div key={dim.dimension} className="app-item">
                      <div className="app-name">
                        {dim.dimension} · +{dim.total_upside.toFixed(2)}
                      </div>
                      <div className="app-path">
                        benchmark = {dim.benchmark_basis} · cardinality {dim.cardinality} ·{" "}
                        {dim.segments.length} segment(s)
                        {dim.other_segments_skipped > 0 &&
                          ` (+${dim.other_segments_skipped} trimmed)`}
                      </div>
                    </div>
                  ))}
                </div>
              )}
              {opportunity.skipped_dimensions.length > 0 && (
                <p style={{ fontSize: "0.75rem", color: "#888", marginTop: "0.5rem" }}>
                  Skipped:{" "}
                  {opportunity.skipped_dimensions.map((s) => `${s.dimension} (${s.reason})`).join("; ")}
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </>
  );
}
