import {
  NodeType,
  TaskConfigWithId,
  TaskType,
  WorkflowTaskConfig,
} from "@/stores/useWorkflow";
import { Button } from "@/components/ui/shadcn/button";
import TruncatedText from "@/components/TruncatedText";
import { useNavigate } from "react-router-dom";
import { headerHeight } from "../../layout/constants";
import {
  Bot,
  CircleAlert,
  CircleHelp,
  Code,
  FileText,
  GitBranch,
  LocateFixed,
  Maximize,
  Minimize,
  RefreshCcw,
  Split,
} from "lucide-react";
import { ReactElement } from "react";
import { TaskRun } from "@/services/types";
import { randomKey } from "@/libs/utils/string";

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

const nodeIconMap: Record<NodeType, ReactElement> = {
  execute_sql: <Code size={14} />,
  loop_sequential: <RefreshCcw size={14} />,
  formatter: <FileText size={14} />,
  agent: <Bot size={14} />,
  workflow: <GitBranch size={14} />,
  conditional: <Split size={14} />,
  "conditional-else": <CircleAlert size={14} />,
  "conditional-if": <CircleHelp size={14} />,
};

type Props = {
  name: string;
  type: NodeType;
  task?: TaskConfigWithId;
  taskRun?: TaskRun;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const NodeHeader = ({
  type,
  name,
  task,
  taskRun,
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
          {taskIcon}
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
              {expanded ? <Minimize size={14} /> : <Maximize size={14} />}
            </Button>
          )}
          {type === TaskType.WORKFLOW && (
            <SubWorkflowNavigateButton
              task={task as WorkflowTaskConfig}
              taskRun={taskRun}
            />
          )}
        </div>
      </div>
    </div>
  );
};

type SubWorkflowNavigateButtonProps = {
  task: WorkflowTaskConfig;
  taskRun?: TaskRun;
};

const SubWorkflowNavigateButton = ({
  task,
  taskRun,
}: SubWorkflowNavigateButtonProps) => {
  const navigate = useNavigate();
  const handleClick = () => {
    const pathb64 = btoa(task.src);
    const runPath = taskRun?.subWorkflowRunId
      ? `/runs/${taskRun.subWorkflowRunId}`
      : "";
    navigate("/workflows/" + pathb64 + runPath + `#${randomKey()}`);
  };

  return (
    <Button
      className="p-1 ps-1 pe-1"
      variant="ghost"
      title="Navigate to definition"
      onClick={handleClick}
    >
      <LocateFixed size={14} />
    </Button>
  );
};
