import { Button } from "@/components/ui/shadcn/button";
import { Badge } from "@/components/ui/shadcn/badge";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";
import { formatTimeAgo } from "../../utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

interface DetailPanelProps {
  point: ClusterMapPoint;
  cluster?: ClusterSummary;
  onClose: () => void;
}

export function DetailPanel({ point, cluster, onClose }: DetailPanelProps) {
  return (
    <div className="h-full border-l overflow-y-auto">
      <DetailPanelHeader onClose={onClose} />
      <div className="p-4 space-y-4">
        <ClassificationSection point={point} cluster={cluster} />
        <InputSection point={point} />
        {point.output && <OutputSection point={point} />}
        {cluster && cluster.clusterId !== -1 && (
          <ClusterInfoSection cluster={cluster} />
        )}
        <ActionsSection traceId={point.traceId} />
      </div>
    </div>
  );
}

function DetailPanelHeader({ onClose }: { onClose: () => void }) {
  return (
    <div className="p-4 border-b flex items-center justify-between">
      <h3 className="font-semibold">Trace Details</h3>
      <Button variant="ghost" size="sm" onClick={onClose}>
        ✕
      </Button>
    </div>
  );
}

interface ClassificationSectionProps {
  point: ClusterMapPoint;
  cluster?: ClusterSummary;
}

function ClassificationSection({ point, cluster }: ClassificationSectionProps) {
  return (
    <div>
      <h4 className="text-xs font-medium text-muted-foreground mb-2">
        Classified as:
      </h4>
      <div className="flex items-center gap-2">
        <Badge
          style={{ backgroundColor: cluster?.color || "#9ca3af" }}
          className="text-white"
        >
          {point.intentName}
        </Badge>
        <span className="text-sm font-mono bg-primary/10 px-2 py-0.5 rounded">
          {(point.confidence * 100).toFixed(2)}%
        </span>
      </div>
    </div>
  );
}

function InputSection({ point }: { point: ClusterMapPoint }) {
  return (
    <div>
      <h4 className="text-xs font-medium text-muted-foreground mb-2 flex items-center gap-2">
        Input
        {point.timestamp && (
          <span className="font-normal">{formatTimeAgo(point.timestamp)}</span>
        )}
      </h4>
      <p className="text-sm bg-muted/50 p-3 rounded-lg">{point.question}</p>
    </div>
  );
}

function OutputSection({ point }: { point: ClusterMapPoint }) {
  return (
    <div>
      <h4 className="text-xs font-medium text-muted-foreground mb-2 flex items-center gap-2">
        Output
        {point.durationMs && (
          <span className="font-normal">+{point.durationMs.toFixed(2)}s</span>
        )}
      </h4>
      <p className="text-sm bg-muted/50 p-3 rounded-lg whitespace-pre-wrap">
        {point.output}
      </p>
    </div>
  );
}

function ClusterInfoSection({ cluster }: { cluster: ClusterSummary }) {
  return (
    <div>
      <h4 className="text-xs font-medium text-muted-foreground mb-2">
        Cluster Info
      </h4>
      <div className="text-sm space-y-2">
        <p>
          <span className="text-muted-foreground">Name:</span>{" "}
          {cluster.intentName}
        </p>
        <p>
          <span className="text-muted-foreground">Description:</span>{" "}
          {cluster.description}
        </p>
        {cluster.sampleQuestions.length > 0 && (
          <SampleQuestions questions={cluster.sampleQuestions} />
        )}
      </div>
    </div>
  );
}

function SampleQuestions({ questions }: { questions: string[] }) {
  return (
    <div>
      <span className="text-muted-foreground">Similar questions:</span>
      <ul className="mt-1 space-y-1">
        {questions.slice(0, 3).map((q, i) => (
          <li key={i} className="text-xs text-muted-foreground pl-2 border-l">
            {q}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ActionsSection({ traceId }: { traceId: string }) {
  const { project } = useCurrentProjectBranch();
  return (
    <div className="pt-4 border-t">
      <Button variant="outline" className="w-full" asChild>
        <a
          href={ROUTES.PROJECT(project.id).IDE.OBSERVABILITY.TRACE(traceId)}
          target="_blank"
          rel="noopener noreferrer"
        >
          View Full Trace
        </a>
      </Button>
    </div>
  );
}
