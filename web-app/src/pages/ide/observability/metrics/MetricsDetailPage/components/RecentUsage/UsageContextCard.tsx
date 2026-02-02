import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Clock, ExternalLink, ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import useCurrentProject from "@/stores/useCurrentProject";
import ROUTES from "@/libs/utils/routes";
import { parseContextItems, getTimeAgo } from "../../utils";
import { CONTEXT_TYPE_CONFIG } from "../../constants";
import SourceTypeBadge from "./SourceTypeBadge";
import ContextItemDisplay from "./ContextItemDisplay";
import HighlightedText from "./HighlightedText";
import type { RecentUsage } from "@/services/api/metrics";

interface UsageContextCardProps {
  usage: RecentUsage;
  metricName: string;
  onTraceClick: (traceId: string) => void;
}

export default function UsageContextCard({
  usage,
  metricName,
  onTraceClick,
}: UsageContextCardProps) {
  const navigate = useNavigate();
  const { project } = useCurrentProject();
  const [isExpanded, setIsExpanded] = useState(false);
  const contextItems = parseContextItems(usage.context);
  const hasContext = contextItems.length > 0;

  // Get a preview from the first non-semantic context item
  const previewItem = contextItems.find(
    (item) => item.type !== "semantic" && typeof item.content === "string",
  );
  const previewContent =
    previewItem && typeof previewItem.content === "string"
      ? previewItem.content
      : null;
  const previewIsSQL = previewItem?.type === "sql";

  return (
    <div
      className={cn(
        "group relative rounded-xl border transition-all duration-200",
        "bg-gradient-to-br from-card to-card/50",
        "hover:border-primary/30 hover:shadow-md hover:shadow-primary/5",
        isExpanded && "border-primary/30",
      )}
    >
      {/* Main content */}
      <div className="p-4 space-y-3">
        {/* Header */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <SourceTypeBadge sourceType={usage.source_type || "agent"} />
            {/* Context type badges */}
            {usage.context_types &&
              usage.context_types.map((ct, idx) => {
                const config = CONTEXT_TYPE_CONFIG[ct];
                if (!config) return null;
                return (
                  <span
                    key={idx}
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
              })}
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Clock className="h-3 w-3" />
            <span>{getTimeAgo(usage.created_at)}</span>
          </div>
        </div>

        {/* Source ref */}
        {usage.source_ref && (
          <div className="text-sm">
            <button
              onClick={() => {
                const pathb64 = btoa(usage.source_ref);
                navigate(ROUTES.PROJECT(project?.id || "").IDE.FILES.FILE(pathb64));
              }}
              className="text-muted-foreground hover:text-primary font-mono text-xs transition-colors underline-offset-4 hover:underline text-left"
            >
              {usage.source_ref}
            </button>
          </div>
        )}

        {/* Quick context preview */}
        {previewContent && !isExpanded && (
          <div
            className={cn(
              "p-2 rounded-lg border",
              previewIsSQL
                ? "bg-muted/50 border-muted"
                : "bg-muted/30 border-muted/50",
            )}
          >
            {previewIsSQL ? (
              <code className="text-xs text-cyan-400 font-mono line-clamp-2">
                <HighlightedText
                  text={previewContent.slice(0, 100)}
                  highlight={metricName}
                />
                {previewContent.length > 100 && "..."}
              </code>
            ) : (
              <p className="text-xs text-muted-foreground line-clamp-2">
                <HighlightedText
                  text={previewContent.slice(0, 150)}
                  highlight={metricName}
                />
                {previewContent.length > 150 && "..."}
              </p>
            )}
          </div>
        )}

        {/* Links row */}
        <div className="flex items-center gap-3 text-xs">
          <button
            onClick={() => onTraceClick(usage.trace_id)}
            className="flex items-center gap-1 text-muted-foreground hover:text-primary transition-colors"
          >
            <ExternalLink className="h-3 w-3" />
            <span className="font-mono">{usage.trace_id.slice(0, 8)}...</span>
          </button>

          {hasContext && (
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="ml-auto flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors"
            >
              {isExpanded ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
              <span>
                {isExpanded ? "Less" : `More (${contextItems.length})`}
              </span>
            </button>
          )}
        </div>
      </div>

      {/* Expanded context */}
      {isExpanded && hasContext && (
        <div className="border-t px-4 py-3 space-y-4 bg-muted/20">
          {contextItems.map((item, idx) => (
            <ContextItemDisplay key={idx} item={item} metricName={metricName} />
          ))}
        </div>
      )}
    </div>
  );
}
