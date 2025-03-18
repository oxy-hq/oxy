import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { LoaderCircle, Workflow } from "lucide-react";
import PageHeader from "@/components/PageHeader";

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
    <PageHeader className="border border-neutral-200 bg-white items-center">
      <div className="py-2 pr-18 flex justify-between items-center flex-1">
        <div className="flex items-center justify-center gap-0.5 flex-1 w-full min-w-0">
          <Workflow width={16} height={16} />
          <span className="text-sm truncate">{relativePath}</span>
        </div>
        <Button
          onClick={onRun}
          disabled={isRunning}
          variant="default"
          content="icon"
          className="absolute right-4"
        >
          {isRunning ? <LoaderCircle className="animate-spin" /> : "Run"}
        </Button>
      </div>
    </PageHeader>
  );
};

export default WorkflowPageHeader;
