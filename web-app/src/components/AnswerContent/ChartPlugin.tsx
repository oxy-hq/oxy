import { visit } from "unist-util-visit";

import type { PhrasingContent, Parent } from "mdast";

export interface TextDirective extends Parent {
  type: "textDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function ChartPlugin() {
  return (tree: TextDirective) => {
    visit(tree, "textDirective", function (node) {
      if (node.name !== "chart") return;

      const data = node.data || (node.data = {});
      const attributes = node.attributes || {};
      const chart_src = attributes.chart_src;

      if (!chart_src) {
        return;
      }

      data.hName = "chart";
      data.hProperties = {
        chart_src,
      };
    });
  };
}

export default ChartPlugin;
