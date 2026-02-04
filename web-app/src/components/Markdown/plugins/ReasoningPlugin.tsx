import type { Parent, PhrasingContent } from "mdast";
import { visit } from "unist-util-visit";

export interface ContainerDirective extends Parent {
  type: "containerDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function ReasoningPlugin() {
  return (tree: ContainerDirective) => {
    visit(tree, "containerDirective", (node) => {
      if (node.name !== "reasoning" && node.name !== "reason") return;
      node.data = {
        ...node.data,
        hName: "reasoning",
        hProperties: {}
      };
    });
  };
}

export default ReasoningPlugin;
