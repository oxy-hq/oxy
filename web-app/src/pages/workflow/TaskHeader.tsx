import useWorkflow, { TaskConfigWithId } from "@/stores/useWorkflow";
import { taskIconMap, taskNameMap } from "./utils";
import { DynamicIcon } from "lucide-react/dynamic";
import { headerHeight } from "./constants";
import { Button } from "@/components/ui/shadcn/button";

type Props = {
  task: TaskConfigWithId;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const TaskHeader = ({
  task,
  expandable,
  expanded,
  onExpandClick,
}: Props) => {
  const taskName = taskNameMap[task.type];
  const taskIcon = taskIconMap[task.type];
  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);
  const onMoreClick = () => {
    setSelectedNodeId(task.id);
  };
  return (
    <div
      className="gap-2 items-center flex w-full min-w-0"
      style={{
        height: headerHeight,
      }}
    >
      <div className="flex items-center min-w-0">
        <div className="flex items-center justify-center p-2 bg-gray-100 rounded-md">
          <DynamicIcon name={taskIcon} />
        </div>
      </div>
      <div className="flex items-center flex-1 min-w-0">
        <div className="flex flex-col gap-1 flex-1 min-w-0">
          <div className="flex items-center">
            <span className="text-sm text-gray-500 truncate">{taskName}</span>
          </div>
          <div className="flex items-center min-w-0">
            <span className="text-sm truncate min-w-0">{task.name}</span>
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
          <Button
            className="p-1 ps-1 pe-1"
            variant="ghost"
            content="text"
            onClick={onMoreClick}
          >
            <DynamicIcon size={14} name="ellipsis-vertical" />
          </Button>
        </div>
      </div>
    </div>
  );
};
