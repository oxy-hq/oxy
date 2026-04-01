import type { Parent, PhrasingContent } from "mdast";
import { visit } from "unist-util-visit";

export interface TextDirective extends Parent {
  type: "textDirective" | "leafDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function handleChartNode(node: TextDirective) {
  if (node.name !== "chart") return;

  if (!node.data) node.data = {};
  const data = node.data;
  const attributes = node.attributes || {};
  const chart_src = attributes.chart_src;

  if (!chart_src) {
    return;
  }

  data.hName = "chart";
  data.hProperties = {
    chart_src
  };
}

function ChartPlugin() {
  return (tree: TextDirective) => {
    visit(tree, "textDirective", handleChartNode);
    visit(tree, "leafDirective", handleChartNode);
  };
}

export default ChartPlugin;
