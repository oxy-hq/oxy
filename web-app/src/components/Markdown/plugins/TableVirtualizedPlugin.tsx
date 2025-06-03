import { visit } from "unist-util-visit";

import type { PhrasingContent, Parent } from "mdast";

export interface TextDirective extends Parent {
  type: "textDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function TableVirtualizedPlugin() {
  return (tree: TextDirective) => {
    visit(tree, "textDirective", function (node) {
      if (node.name !== "table_virtualized") return;

      const data = node.data || (node.data = {});
      const attributes = node.attributes || {};
      const table_id = attributes.table_id;

      if (!table_id) {
        return;
      }

      data.hName = "table_virtualized";
      data.hProperties = {
        table_id: table_id,
      };
    });
  };
}

export default TableVirtualizedPlugin;
