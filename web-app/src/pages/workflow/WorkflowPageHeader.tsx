import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { LoaderCircle } from "lucide-react";

type WorkflowPageHeaderProps = {
  path: string;
  onRun: () => void;
  isRunning: boolean;
};

const WorkflowPageHeader: React.FC<WorkflowPageHeaderProps> = ({
  path,
  onRun,
  isRunning,
}) => {
  const relativePath = path;
  return (
    <div className="p-2 border border-neutral-200 bg-white flex justify-between items-center">
      <span className="text-sm font-medium">{relativePath}</span>
      <Button
        onClick={onRun}
        disabled={isRunning}
        variant="default"
        content="icon"
      >
        {isRunning ? <LoaderCircle className="animate-spin" /> : "Run"}
      </Button>
    </div>
  );
};

export default WorkflowPageHeader;
