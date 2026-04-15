import type { Text } from "mdast";
import type { Parent } from "unist";
import { SKIP, visit } from "unist-util-visit";
import type { TextDirective } from "./types";

/**
 * Must run AFTER all directive-handling plugins. Any textDirective/leafDirective
 * node left without a `data.hName` would be rendered as an unknown HTML tag and
 * stripped by rehype-sanitize, silently swallowing content like `:58` in
 * `**08:58 UTC**`. This converts those unclaimed nodes back to plain text.
 */
function DirectiveFallbackPlugin() {
  return (tree: Parent) => {
    visit(tree, ["textDirective", "leafDirective"], (node, index, parent) => {
      const directive = node as unknown as TextDirective;
      if (directive.data?.hName || !parent) return;

      let text = `:${directive.name}`;
      if (directive.children.length > 0) {
        const label = directive.children
          .map((c) => (c.type === "text" ? (c as Text).value : ""))
          .join("");
        text += `[${label}]`;
      }

      const textNode: Text = { type: "text", value: text };
      (parent as Parent & { children: unknown[] }).children.splice(index!, 1, textNode);
      return [SKIP, index! + 1];
    });
  };
}

export default DirectiveFallbackPlugin;
