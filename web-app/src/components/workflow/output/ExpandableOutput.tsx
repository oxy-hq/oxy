import React from "react";
import { DynamicIcon } from "lucide-react/dynamic";
import { Button } from "@/components/ui/shadcn/button";
import Markdown from "@/components/Markdown";

const getFirstLine = (content: string) => {
  const lines = content.split("\n");
  // return the first line that's not empty
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() !== "") {
      return lines[i];
    }
  }
  return "";
};

type ExpandableOutputProps = {
  content: string;
  timestamp: string;
  onArtifactClick?: (id: string) => void;
};
const ExpandableOutput = ({
  content,
  timestamp,
  onArtifactClick,
}: ExpandableOutputProps) => {
  const [expanded, setExpanded] = React.useState(true);
  const firstLine = getFirstLine(content);
  const toggle = () => {
    setExpanded(!expanded);
  };
  if (!expanded) {
    return (
      <div
        className="flex w-full border-b border-border cursor-pointer"
        onClick={toggle}
      >
        <div className="w-10 flex items-center justify-center">
          <Button variant="link" content="icon">
            <DynamicIcon name="chevron-down" size={14} />
          </Button>
        </div>
        <div className="p-2 flex-1 text-xs flex justify-between items-center ">
          <span>{firstLine}</span>
          <span className="text-gray-400 text-xs flex justify-end">
            {timestamp}
          </span>
        </div>
      </div>
    );
  }
  return (
    <div
      className="border-b border-border flex justify-between items-stretch cursor-pointer"
      onClick={toggle}
    >
      <div className="w-10 flex items-start justify-center">
        <Button variant="link" content="icon">
          <DynamicIcon name="chevron-up" size={14} />
        </Button>
      </div>
      <div className="p-2 pt-3 flex-1 text-xs overflow-hidden">
        <Markdown onArtifactClick={onArtifactClick}>{content}</Markdown>
      </div>
    </div>
  );
};

export default ExpandableOutput;
