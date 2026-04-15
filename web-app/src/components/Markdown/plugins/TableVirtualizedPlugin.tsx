import { visit } from "unist-util-visit";
import type { TextDirective } from "./types";

function TableVirtualizedPlugin() {
  return (tree: TextDirective) => {
    visit(tree, "textDirective", (node) => {
      if (node.name !== "table_virtualized") return;

      const attributes = node.attributes || {};
      const table_id = attributes.table_id;

      if (!table_id) {
        return;
      }

      node.data = {
        ...node.data,
        hName: "table_virtualized",
        hProperties: { table_id }
      };
    });
  };
}

export default TableVirtualizedPlugin;
