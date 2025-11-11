import { LogItem } from "@/services/types";
import Markdown from "@/components/Markdown";
import { ChevronDown, ChevronRight } from "lucide-react";

type OutputItemProps = {
  onArtifactClick?: (id: string) => void;
  log: LogItem;
  isPending?: boolean;
  depth?: number;
  isExpandable?: boolean;
  isExpanded?: boolean;
  onToggleExpanded?: () => void;
};

const OutputItem = ({
  depth = 0,
  log,
  onArtifactClick,
  isExpandable = false,
  isExpanded = false,
  onToggleExpanded,
}: OutputItemProps) => {
  if (isExpandable && onToggleExpanded) {
    return (
      <div
        className="w-full min-w-[500px]"
        style={{ paddingLeft: depth > 0 ? `${depth * 24}px` : undefined }}
      >
        <div
          className="w-full flex items-center justify-center py-2 gap-2 cursor-pointer"
          onClick={onToggleExpanded}
        >
          {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}

          <div className="flex-1 text-sm flex justify-between items-center">
            <span>{log.content}</span>
            <span className="text-gray-400 text-xs flex justify-end">
              {log.timestamp}
            </span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="min-w-[500px]"
      style={{
        paddingLeft: depth > 0 ? `${depth * 24}px` : undefined,
      }}
    >
      <Markdown onArtifactClick={onArtifactClick}>{log.content}</Markdown>
    </div>
  );
};

export default OutputItem;
