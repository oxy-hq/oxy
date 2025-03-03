import { SvgAssets } from "@/components/ui/Icon/Dictionary";
import { TaskType } from "@/stores/useWorkflow";

export const taskNameMap: Record<TaskType, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
};

export const taskIconMap: Record<TaskType, SvgAssets> = {
  execute_sql: "code",
  loop_sequential: "arrow_reload",
  formatter: "placeholder",
  agent: "agent",
};
