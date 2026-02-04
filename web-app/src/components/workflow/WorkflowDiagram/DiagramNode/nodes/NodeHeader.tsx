import {
  Bot,
  CircleAlert,
  CircleHelp,
  Code,
  FileText,
  GitBranch,
  Globe,
  LocateFixed,
  Maximize,
  Minimize,
  RefreshCcw,
  Split
} from "lucide-react";
import type { ReactElement } from "react";
import { createSearchParams, useLocation, useNavigate } from "react-router-dom";
import TruncatedText from "@/components/TruncatedText";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import type { TaskRun } from "@/services/types";
import {
  type NodeType,
  type TaskConfigWithId,
  TaskType,
  type WorkflowTaskConfig
} from "@/stores/useWorkflow";
import { headerHeight } from "../../layout/constants";
import { OmniIcon } from "./OmniIcon";

const nodeNameMap: Record<NodeType, string> = {
  execute_sql: "SQL",
  semantic_query: "Semantic Query",
  omni_query: "Omni Query",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
  workflow: "Subworkflow",
  conditional: "Conditional",
  "conditional-else": "Else",
  "conditional-if": "If"
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
  semantic_query: <Globe size={14} />,
  omni_query: <OmniIcon className='h-[14px] w-[14px]' />
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
  onExpandClick
}: Props) => {
  const taskName = nodeNameMap[type];
  const taskIcon = nodeIconMap[type];
  return (
    <div
      className='flex w-full min-w-0 items-center gap-2'
      style={{
        height: headerHeight
      }}
    >
      <div className='flex min-w-0 items-center'>
        <div className='flex items-center justify-center rounded-md bg-special p-2'>{taskIcon}</div>
      </div>
      <div className='flex min-w-0 flex-1 items-center'>
        <div className='flex min-w-0 flex-1 flex-col gap-1'>
          <div className='flex items-center'>
            <span className='truncate text-gray-500 text-sm'>{taskName}</span>
          </div>
          <div className='flex min-w-0 items-center'>
            <TruncatedText className='min-w-0 text-sm'>{name}</TruncatedText>
          </div>
        </div>
        <div className='flex h-full items-center justify-start'>
          {expandable && (
            <Button className='p-1 ps-1 pe-1' variant='ghost' onClick={onExpandClick}>
              {expanded ? <Minimize size={14} /> : <Maximize size={14} />}
            </Button>
          )}
          {type === TaskType.WORKFLOW && (
            <SubWorkflowNavigateButton task={task as WorkflowTaskConfig} taskRun={taskRun} />
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

const SubWorkflowNavigateButton = ({ task, taskRun }: SubWorkflowNavigateButtonProps) => {
  const location = useLocation();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const handleClick = () => {
    if (!projectId) return;

    const pathb64 = btoa(task.src);

    let workflowPath = ROUTES.PROJECT(projectId).WORKFLOW(pathb64).ROOT;

    const ideRoute = ROUTES.PROJECT(projectId).IDE.ROOT;
    if (location.pathname.startsWith(ideRoute)) {
      workflowPath = ROUTES.PROJECT(projectId).IDE.FILES.FILE(pathb64);
    }
    navigate({
      pathname: workflowPath,
      search: createSearchParams({
        run: taskRun?.subWorkflowRunId?.toString() || ""
      }).toString()
    });
  };

  return (
    <Button
      className='p-1 ps-1 pe-1'
      variant='ghost'
      title='Navigate to definition'
      onClick={handleClick}
    >
      <LocateFixed size={14} />
    </Button>
  );
};
