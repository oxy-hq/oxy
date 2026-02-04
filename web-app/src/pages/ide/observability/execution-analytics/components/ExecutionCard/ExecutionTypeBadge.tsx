import { Database, Layers, LucideBot, LucideWorkflow, Plug } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { EXECUTION_TYPES, type ExecutionDetail } from "../../types";

const TYPE_ICONS: Record<ExecutionDetail["executionType"], typeof Database> = {
  semantic_query: Layers,
  omni_query: Plug,
  sql_generated: Database,
  workflow: LucideWorkflow,
  agent_tool: LucideBot
};

interface ExecutionTypeBadgeProps {
  executionType: ExecutionDetail["executionType"];
}

export default function ExecutionTypeBadge({ executionType }: ExecutionTypeBadgeProps) {
  const typeInfo = EXECUTION_TYPES[executionType];
  const TypeIcon = TYPE_ICONS[executionType] ?? Database;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-medium text-xs",
        typeInfo.bgColor,
        typeInfo.color
      )}
    >
      <TypeIcon className='h-3.5 w-3.5' />
      {typeInfo.label}
    </span>
  );
}
