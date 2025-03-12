import { TaskType } from "@/stores/useWorkflow";
import { IconName } from "lucide-react/dynamic";

export const taskNameMap: Record<TaskType, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
  workflow: "Subworkflow",
};

export const taskIconMap: Record<TaskType, IconName> = {
  execute_sql: "code",
  loop_sequential: "refresh-ccw",
  formatter: "file-text",
  agent: "bot",
  workflow: "git-branch",
};
