import { NodeType } from "@/stores/useWorkflow";
import { IconName } from "lucide-react/dynamic";

export const nodeNameMap: Record<NodeType, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
  workflow: "Subworkflow",
  conditional: "Conditional",
  "conditional-else": "Else",
  "conditional-if": "If",
};

export const nodeIconMap: Record<NodeType, IconName> = {
  execute_sql: "code",
  loop_sequential: "refresh-ccw",
  formatter: "file-text",
  agent: "bot",
  workflow: "git-branch",
  conditional: "split",
  "conditional-else": "circle-alert",
  "conditional-if": "circle-help",
};
