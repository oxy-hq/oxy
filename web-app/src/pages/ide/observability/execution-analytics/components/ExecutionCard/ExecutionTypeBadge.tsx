import { Layers, Database, LucideWorkflow, Plug, LucideBot } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { ExecutionDetail, EXECUTION_TYPES } from "../../types";

const TYPE_ICONS: Record<ExecutionDetail["executionType"], typeof Database> = {
  semantic_query: Layers,
  omni_query: Plug,
  sql_generated: Database,
  workflow: LucideWorkflow,
  agent_tool: LucideBot,
};

interface ExecutionTypeBadgeProps {
  executionType: ExecutionDetail["executionType"];
}

export default function ExecutionTypeBadge({
  executionType,
}: ExecutionTypeBadgeProps) {
  const typeInfo = EXECUTION_TYPES[executionType];
  const TypeIcon = TYPE_ICONS[executionType] ?? Database;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border",
        typeInfo.bgColor,
        typeInfo.color,
      )}
    >
      <TypeIcon className="h-3.5 w-3.5" />
      {typeInfo.label}
    </span>
  );
}
