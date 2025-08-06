import { cx } from "class-variance-authority";
import ExpandableOutput from "./ExpandableOutput";
import { LogType } from "@/services/types";

const getLogColor = (logType: LogType) => {
  switch (logType) {
    case "info":
      return "";
    case "error":
      return "text-red-500";
    case "warning":
      return "text-yellow-500";
    case "success":
      return "text-green-500";
    default:
      return "text-gray-500";
  }
};

type OutputItemProps = {
  content: string;
  timestamp: string;
  logType: LogType;
  onArtifactClick?: (id: string) => void;
};

const OutputItem = ({
  content = "",
  timestamp,
  logType,
  onArtifactClick,
}: OutputItemProps) => {
  const lineNumbers = content.split("\n").length;
  if (lineNumbers > 1) {
    return (
      <ExpandableOutput
        onArtifactClick={onArtifactClick}
        content={content}
        timestamp={timestamp}
      />
    );
  }

  return (
    <div className="border-b p-2 border-border flex justify-between items-stretch text-xs">
      <div className="w-10"></div>
      <span className="flex-1 flex justify-between items-center">
        <span className={cx("flex-1", getLogColor(logType))}>{content}</span>
      </span>
      <span className="text-background-foreground text-xs flex justify-end">
        {timestamp}
      </span>
    </div>
  );
};

export default OutputItem;
