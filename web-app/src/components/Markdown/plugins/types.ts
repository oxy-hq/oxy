import type { PhrasingContent } from "mdast";
import type { Parent } from "unist";

export interface TextDirective extends Parent {
  type: "textDirective" | "leafDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
  data?: Record<string, unknown>;
}

export interface ContainerDirective extends Parent {
  type: "containerDirective";
  name: string;
  attributes?: Record<string, string | null | undefined> | null | undefined;
  children: Array<PhrasingContent>;
  data?: Record<string, unknown>;
}
