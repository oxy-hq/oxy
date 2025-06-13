import {
  NodeType,
  TaskConfigWithId,
  TaskType,
  WorkflowTaskConfig,
} from "@/stores/useWorkflow";
import { DynamicIcon } from "lucide-react/dynamic";
import { Button } from "@/components/ui/shadcn/button";
import TruncatedText from "@/components/TruncatedText";
import { useNavigate } from "react-router-dom";
import { headerHeight } from "../../layout/constants";
import { IconName } from "lucide-react/dynamic";

const nodeNameMap: Record<NodeType, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
  workflow: "Subworkflow",
  conditional: "Conditional",
  "conditional-else": "Else",
  "conditional-if": "If",
};

const nodeIconMap: Record<NodeType, IconName> = {
  execute_sql: "code",
  loop_sequential: "refresh-ccw",
  formatter: "file-text",
  agent: "bot",
  workflow: "git-branch",
  conditional: "split",
  "conditional-else": "circle-alert",
  "conditional-if": "circle-help",
};

type Props = {
  name: string;
  type: NodeType;
  task?: TaskConfigWithId;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const NodeHeader = ({
  type,
  name,
  task,
  expandable,
  expanded,
  onExpandClick,
}: Props) => {
  const taskName = nodeNameMap[type];
  const taskIcon = nodeIconMap[type];
  return (
    <div
      className="gap-2 items-center flex w-full min-w-0"
      style={{
        height: headerHeight,
      }}
    >
      <div className="flex items-center min-w-0">
        <div className="flex items-center justify-center p-2 bg-special rounded-md">
          <DynamicIcon name={taskIcon} />
        </div>
      </div>
      <div className="flex items-center flex-1 min-w-0">
        <div className="flex flex-col gap-1 flex-1 min-w-0">
          <div className="flex items-center">
            <span className="text-sm text-gray-500 truncate">{taskName}</span>
          </div>
          <div className="flex items-center min-w-0">
            <TruncatedText className="text-sm min-w-0">{name}</TruncatedText>
          </div>
        </div>
        <div className="flex items-center h-full justify-start">
          {expandable && (
            <Button
              className="p-1 ps-1 pe-1"
              variant="ghost"
              onClick={onExpandClick}
            >
              <DynamicIcon
                size={14}
                name={expanded ? "minimize-2" : "maximize-2"}
              />
            </Button>
          )}
          {type === TaskType.WORKFLOW && (
            <SubWorkflowNavigateButton task={task as WorkflowTaskConfig} />
          )}
        </div>
      </div>
    </div>
  );
};

type SubWorkflowNavigateButtonProps = {
  task: WorkflowTaskConfig;
};

const SubWorkflowNavigateButton = ({
  task,
}: SubWorkflowNavigateButtonProps) => {
  const navigate = useNavigate();
  const handleClick = () => {
    const pathb64 = btoa(task.src);
    navigate("/workflows/" + pathb64);
  };

  return (
    <Button
      className="p-1 ps-1 pe-1"
      variant="ghost"
      title="Navigate to definition"
      onClick={handleClick}
    >
      <DynamicIcon size={14} name="locate-fixed" />
    </Button>
  );
};
