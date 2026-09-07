import type { MetricTree } from "@/types/metricTree";

export type NodeRole = "composite" | "component" | "driver" | "leaf";

/**
 * How a role is drawn on a card: a symbol plus an uppercase micro-label, the
 * same way the World Model marks a measure's type.
 *
 * Role deliberately does NOT colour the card's border or background. Every
 * semantic-graph card shares one accent so that colour means *selection state*
 * on both surfaces rather than meaning role here and cluster membership there —
 * see `GraphNodeCard`.
 */
export const ROLE_MARKS: Record<NodeRole, { symbol: string; label: string; className: string }> = {
  composite: { symbol: "Σ", label: "composite", className: "text-[color:var(--vis-purple)]" },
  component: { symbol: "+", label: "component", className: "text-success" },
  driver: { symbol: "→", label: "driver", className: "text-info" },
  leaf: { symbol: "·", label: "leaf", className: "text-muted-foreground" }
};

export function deriveNodeRoles(tree: MetricTree): Map<string, NodeRole> {
  const roles = new Map<string, NodeRole>();
  const componentTargets = new Set(
    tree.edges.filter((e) => e.kind === "component").map((e) => e.to)
  );
  const driverSources = new Set(tree.edges.filter((e) => e.kind === "driver").map((e) => e.from));

  for (const node of tree.nodes) {
    if (node.is_composite || componentTargets.has(node.id)) {
      roles.set(node.id, "composite");
    } else if (driverSources.has(node.id)) {
      roles.set(node.id, "driver");
    } else if (tree.edges.some((e) => e.to === node.id && e.kind === "component")) {
      roles.set(node.id, "component");
    } else {
      roles.set(node.id, "leaf");
    }
  }
  return roles;
}
