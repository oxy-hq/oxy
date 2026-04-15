import type { Root } from "mdast";
import { visit } from "unist-util-visit";

interface ChartDirectiveNode {
  type: string;
  name: string;
  attributes?: Record<string, string | null | undefined> | null;
  data?: Record<string, unknown>;
}

function handleChartNode(node: ChartDirectiveNode) {
  if (node.name !== "chart") return;

  const attributes = node.attributes || {};
  const chart_src = attributes.chart_src;

  if (!chart_src) {
    return;
  }

  node.data = {
    ...node.data,
    hName: "chart",
    hProperties: { chart_src }
  };
}

function ChartPlugin() {
  return (tree: Root) => {
    visit(tree, "textDirective", (node) => handleChartNode(node as unknown as ChartDirectiveNode));
    visit(tree, "leafDirective", (node) => handleChartNode(node as unknown as ChartDirectiveNode));
  };
}

export default ChartPlugin;
