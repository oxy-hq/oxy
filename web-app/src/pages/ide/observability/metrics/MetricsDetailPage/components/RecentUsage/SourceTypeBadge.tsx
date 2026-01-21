import { cn } from "@/libs/shadcn/utils";
import { SOURCE_TYPE_CONFIG } from "../../constants";

interface SourceTypeBadgeProps {
  sourceType: string;
}

export default function SourceTypeBadge({ sourceType }: SourceTypeBadgeProps) {
  const config = SOURCE_TYPE_CONFIG[sourceType] || SOURCE_TYPE_CONFIG.agent;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border",
        config.bgColor,
        config.color,
      )}
    >
      {config.icon}
      {config.label}
    </span>
  );
}
