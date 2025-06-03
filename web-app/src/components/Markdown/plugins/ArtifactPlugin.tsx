import { visit } from "unist-util-visit";

import type { PhrasingContent, Parent } from "mdast";

export interface ContainerDirective extends Parent {
  type: "containerDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function ArtifactPlugin() {
  return (tree: ContainerDirective) => {
    visit(tree, "containerDirective", function (node) {
      if (node.name !== "artifact") return;

      const attributes = node.attributes || {};
      const kind = attributes.kind;
      const title = attributes.title;
      const is_verified = attributes.is_verified;

      if (!kind) {
        return;
      }
      node.data = {
        ...node.data,
        hName: "artifact",
        hProperties: {
          kind,
          title,
          is_verified,
        },
      };
    });
  };
}

export default ArtifactPlugin;
