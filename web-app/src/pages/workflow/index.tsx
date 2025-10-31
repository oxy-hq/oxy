import React, { useMemo } from "react";
import { useLocation, useParams, useSearchParams } from "react-router-dom";
import WorkflowPageHeader from "./Header";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";

export const Workflow: React.FC<{ pathb64: string; runId?: string }> = ({
  pathb64,
  runId,
}) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const location = useLocation();
  const hashValue = location.hash;

  return (
    <div className="w-full h-full flex flex-col">
      <WorkflowPageHeader path={path} runId={runId} />
      <WorkflowPreview
        key={`${pathb64}-${runId}-${hashValue}`}
        pathb64={pathb64}
        runId={runId}
      />
    </div>
  );
};

const WorkflowPage = () => {
  const { pathb64 } = useParams();
  const [searchParams] = useSearchParams();
  const runId = searchParams.get("run") || undefined;
  return (
    <Workflow
      key={`${pathb64}-${runId}`}
      pathb64={pathb64 ?? ""}
      runId={runId}
    />
  );
};

export default WorkflowPage;
