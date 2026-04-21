import { cn } from "@/libs/shadcn/utils";
import { SOURCE_TYPE_CONFIG } from "../../constants";

interface SourceTypeBadgeProps {
  sourceType: string;
}

const FALLBACK_CONFIG = {
  label: "Unknown",
  color: "text-muted-foreground",
  bgColor: "bg-muted border-border",
  icon: null as React.ReactNode
};

export default function SourceTypeBadge({ sourceType }: SourceTypeBadgeProps) {
  // Lookup is case-insensitive — the backend writes lowercase since
  // SourceType::as_str was normalized, but older rows may still use
  // PascalCase. Fall back to a neutral "Unknown" badge so a novel
  // `source_type` value can never crash the detail page.
  const config =
    SOURCE_TYPE_CONFIG[sourceType] ||
    SOURCE_TYPE_CONFIG[sourceType.toLowerCase()] ||
    FALLBACK_CONFIG;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-medium text-xs",
        config.bgColor,
        config.color
      )}
    >
      {config.icon}
      {config.label || sourceType}
    </span>
  );
}
