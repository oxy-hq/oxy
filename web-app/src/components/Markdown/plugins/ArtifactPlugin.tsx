import type { Parent, PhrasingContent } from "mdast";
import { visit } from "unist-util-visit";

export interface ContainerDirective extends Parent {
  type: "containerDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
}

function ArtifactPlugin() {
  return (tree: ContainerDirective) => {
    visit(tree, "containerDirective", (node) => {
      if (node.name !== "artifact") return;

      const attributes = node.attributes || {};
      const kind = attributes.kind;
      const title = attributes.title;
      const is_verified = attributes.is_verified;
      const id = attributes.id ?? "";

      if (!kind) {
        return;
      }
      node.data = {
        ...node.data,
        hName: "artifact",
        hProperties: {
          artifactId: id,
          kind,
          title,
          is_verified
        }
      };
    });
  };
}

export default ArtifactPlugin;
