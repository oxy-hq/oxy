import { visit } from "unist-util-visit";
import type { ContainerDirective } from "./types";

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
