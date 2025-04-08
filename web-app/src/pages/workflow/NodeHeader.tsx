import { NodeType } from "@/stores/useWorkflow";
import { nodeIconMap, nodeNameMap } from "./utils";
import { DynamicIcon } from "lucide-react/dynamic";
import { headerHeight } from "./constants";
import { Button } from "@/components/ui/shadcn/button";
import TruncatedText from "@/components/TruncatedText";

type Props = {
  name: string;
  type: NodeType;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const NodeHeader = ({
  type,
  name,
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
        </div>
      </div>
    </div>
  );
};
