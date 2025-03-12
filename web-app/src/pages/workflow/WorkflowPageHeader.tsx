import React from "react";
import useProjectPath from "@/stores/useProjectPath";
import Text from "@/components/ui/Typography/Text";
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
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = path.replace(projectPath, "").replace(/^\//, "");
  return (
    <div className="p-2 border border-neutral-200 bg-white flex justify-between items-center">
      <Text variant="bodyBaseMedium">{relativePath}</Text>
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
