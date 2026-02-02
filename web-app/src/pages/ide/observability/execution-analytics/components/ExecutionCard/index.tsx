import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ChevronDown, ChevronRight, ExternalLink, Clock } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { formatDistanceToNow } from "date-fns";
import useCurrentProject from "@/stores/useCurrentProject";
import ROUTES from "@/libs/utils/routes";
import { ExecutionDetail } from "../../types";

import ExecutionTypeBadge from "./ExecutionTypeBadge";
import StatusBadge from "./StatusBadge";
import VerifiedBadge from "./VerifiedBadge";
import SqlDisplay from "./SqlDisplay";
import ErrorDisplay from "./ErrorDisplay";
import DataDisplay from "./DataDisplay";

interface ExecutionCardProps {
  execution: ExecutionDetail;
}

function formatDuration(ms: number): string {
  if (ms < 1) return "<1ms";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export default function ExecutionCard({ execution }: ExecutionCardProps) {
  const navigate = useNavigate();
  const { project } = useCurrentProject();
  const [isExpanded, setIsExpanded] = useState(false);

  const handleViewTrace = (traceId: string) => {
    navigate(`/ide/observability/traces/${traceId}`);
  };

  const isSuccess = execution.status === "success";
  const sql = execution.sql || execution.generatedSql || "";
  const hasSql = !!sql;
  const hasExpandableContent =
    hasSql || execution.error || execution.output || execution.toolInput;

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
            <ExecutionTypeBadge executionType={execution.executionType} />
            <VerifiedBadge isVerified={execution.isVerified} />
            <StatusBadge isSuccess={isSuccess} />
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Clock className="h-3 w-3" />
            <span>
              {formatDistanceToNow(new Date(execution.timestamp), {
                addSuffix: true,
              })}
            </span>
          </div>
        </div>

        {execution.sourceRef && (
          <div className="text-sm">
            <button
              onClick={() => {
                const pathb64 = btoa(execution.sourceRef);
                navigate(ROUTES.PROJECT(project?.id || "").IDE.FILES.FILE(pathb64));
              }}
              className="text-muted-foreground hover:text-primary font-mono text-xs transition-colors underline-offset-4 hover:underline text-left"
            >
              {execution.sourceRef}
            </button>
          </div>
        )}

        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          {execution.topic && (
            <span>
              <span className="text-muted-foreground">Topic:</span>{" "}
              <span className="font-medium text-foreground">
                {execution.topic}
              </span>
            </span>
          )}
          {execution.database && (
            <span>
              <span className="text-muted-foreground">Database:</span>{" "}
              <span className="font-medium text-foreground">
                {execution.database}
              </span>
            </span>
          )}
          {execution.workflowRef && (
            <span>
              <span className="text-muted-foreground">Workflow ref:</span>{" "}
              <span className="font-medium text-foreground">
                <button
                  onClick={() => {
                    const pathb64 = btoa(execution.workflowRef || "");
                    navigate(
                      ROUTES.PROJECT(project?.id || "").IDE.FILES.FILE(pathb64),
                    );
                  }}
                  className="hover:text-primary font-mono text-xs transition-colors underline-offset-4 hover:underline text-left"
                >
                  {execution.workflowRef}
                </button>
              </span>
            </span>
          )}
          {execution.agentRef && (
            <span>
              <span className="text-muted-foreground">Agent ref:</span>{" "}
              <span className="font-medium text-foreground">
                <button
                  onClick={() => {
                    const pathb64 = btoa(execution.agentRef || "");
                    navigate(
                      ROUTES.PROJECT(project?.id || "").IDE.FILES.FILE(pathb64),
                    );
                  }}
                  className="hover:text-primary font-mono text-xs transition-colors underline-offset-4 hover:underline text-left"
                >
                  {execution.agentRef}
                </button>
              </span>
            </span>
          )}
          {execution.integration && (
            <span>
              <span className="text-muted-foreground">Integration:</span>{" "}
              <span className="font-medium text-foreground">
                {execution.integration}
              </span>
            </span>
          )}
          {execution.endpoint && (
            <span>
              <span className="text-muted-foreground">Endpoint:</span>{" "}
              <span className="font-medium text-foreground">
                {execution.endpoint}
              </span>
            </span>
          )}
          <span>
            <span className="text-muted-foreground">Duration:</span>{" "}
            <span className="font-medium text-foreground">
              {formatDuration(execution.durationMs)}
            </span>
          </span>
        </div>

        {/* Quick previews when collapsed */}
        {!isExpanded && hasSql && <SqlDisplay sql={sql} isPreview />}

        <div className="flex items-center gap-3 text-xs">
          <button
            onClick={() => handleViewTrace(execution.traceId)}
            className="flex items-center gap-1 text-muted-foreground hover:text-primary transition-colors"
          >
            <ExternalLink className="h-3 w-3" />
            <span className="font-mono">
              {execution.traceId.slice(0, 8)}...
            </span>
          </button>

          {hasExpandableContent && (
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="ml-auto flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors"
            >
              {isExpanded ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
              <span>{isExpanded ? "Less" : "More"}</span>
            </button>
          )}
        </div>
      </div>

      {isExpanded && hasExpandableContent && (
        <div className="border-t px-4 py-3 space-y-4 bg-muted/20">
          {hasSql && (
            <SqlDisplay
              sql={sql}
              label={
                execution.executionType === "semantic_query"
                  ? "Generated SQL"
                  : "SQL Query"
              }
            />
          )}

          {execution.semanticQueryParams && (
            <DataDisplay
              value={execution.semanticQueryParams}
              label="Semantic Query Parameters"
            />
          )}

          {execution.toolInput && (
            <DataDisplay value={execution.toolInput} label="Input" />
          )}

          {execution.error && <ErrorDisplay error={execution.error} />}

          {execution.output && (
            <DataDisplay value={execution.output} label="Output" />
          )}
        </div>
      )}
    </div>
  );
}
