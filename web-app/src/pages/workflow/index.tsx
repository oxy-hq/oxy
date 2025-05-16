import React, { useMemo } from "react";
import { useParams } from "react-router-dom";
import WorkflowPageHeader from "./WorkflowPageHeader";
import { WorkflowPreview } from "./WorkflowPreview";

export const Workflow: React.FC<{ pathb64: string }> = ({ pathb64 }) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);

  return (
    <div className="w-full h-full flex flex-col">
      <WorkflowPageHeader path={path} />
      <WorkflowPreview pathb64={pathb64} />
    </div>
  );
};

const WorkflowPage = () => {
  const { pathb64 } = useParams();
  return <Workflow key={pathb64 ?? ""} pathb64={pathb64 ?? ""} />;
};

export default WorkflowPage;
