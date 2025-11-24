import { visit } from "unist-util-visit";

import type { PhrasingContent, Parent } from "mdast";

export interface ContainerDirective extends Parent {
  type: "containerDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function ReasoningPlugin() {
  return (tree: ContainerDirective) => {
    visit(tree, "containerDirective", function (node) {
      if (node.name !== "reasoning" && node.name !== "reason") return;
      node.data = {
        ...node.data,
        hName: "reasoning",
        hProperties: {},
      };
    });
  };
}

export default ReasoningPlugin;
