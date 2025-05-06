import React from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import directive from "remark-directive";
import CodeBlock from "@/components/CodeBlock";
import { DynamicIcon } from "lucide-react/dynamic";
import { Button } from "@/components/ui/shadcn/button";

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
};

const ExpandableOutput = ({ content, timestamp }: ExpandableOutputProps) => {
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
      <div className="pb-2 pl-2 pr-2 flex-1 text-xs">
        <MarkdownContent content={content}></MarkdownContent>
      </div>
    </div>
  );
};

type MarkdownContentProps = {
  content: string;
};

export const MarkdownContent = ({ content }: MarkdownContentProps) => {
  return (
    <div className="markdown">
      <Markdown
        components={{
          code: CodeBlock,
        }}
        remarkPlugins={[remarkGfm, directive]}
      >
        {content}
      </Markdown>
    </div>
  );
};

export default ExpandableOutput;
