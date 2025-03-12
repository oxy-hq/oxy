import React from "react";
import { cx } from "styled-system/css";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import directive from "remark-directive";
import CodeBlock from "@/components/CodeBlock";
import { DynamicIcon } from "lucide-react/dynamic";
import { LoaderIcon } from "lucide-react";
import { LogItem, LogType } from "@/hooks/api/runWorkflow";
import { Button } from "@/components/ui/shadcn/button";
import dayjs from "dayjs";

interface WorkflowOutputProps {
  showOutput: boolean;
  toggleOutput: () => void;
  isPending: boolean;
  outputEnd: React.RefObject<HTMLDivElement | null>;
  logs: LogItem[];
}

const WorkflowOutput: React.FC<WorkflowOutputProps> = ({
  showOutput,
  toggleOutput,
  isPending,
  outputEnd,
  logs,
}) => {
  return (
    logs.length > 0 && (
      <div
        className="sticky bottom-0"
        style={{
          width: "inherit",
        }}
      >
        <div
          className="px-2 py-1 border border-neutral-200 bg-white flex justify-between items-center"
          style={{
            width: "inherit",
          }}
        >
          <span className="text-gray-700 text-sm">Output</span>
          <Button variant="ghost" content="icon" onClick={toggleOutput}>
            {showOutput ? (
              <DynamicIcon name="x" size={14} />
            ) : (
              <DynamicIcon name="chevron-up" size={14} />
            )}
          </Button>
        </div>
        {showOutput && (
          <div
            className={cx(
              "max-h-75 min-h-50 overflow-scroll scrollbar-thin break-all p-1",
            )}
          >
            <div>
              {logs.map((log, index) => {
                console.log(log);
                return (
                  <OutputItem
                    key={index}
                    content={log.content}
                    timestamp={dayjs(log.timestamp).format(
                      "ddd YYYY-MM-DD H:mm:ss",
                    )}
                    logType={log.log_type}
                  />
                );
              })}
            </div>
            {isPending && (
              <div className="mt-2 flex justify-center">
                <LoaderIcon className="animate-spin" />
              </div>
            )}
            <div ref={outputEnd} />
          </div>
        )}
      </div>
    )
  );
};

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
      <div className="flex w-full border-b ">
        <div className="w-10 flex items-center justify-center">
          <Button variant="link" content="icon" onClick={toggle}>
            <DynamicIcon name="chevron-down" size={14} />
          </Button>
        </div>
        <div
          className="p-2 flex-1 text-xs flex justify-between items-center"
          onClick={toggle}
        >
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
      className="border-b border-neutral-200 bg-white flex justify-between items-stretch"
      onClick={toggle}
    >
      <div className="w-10 flex items-start justify-center">
        <Button variant="link" content="icon" onClick={toggle}>
          <DynamicIcon name="chevron-up" size={14} />
        </Button>
      </div>
      <div className="pb-2 pl-2 pr-2 flex-1 text-xs">
        <MarkdownContent content={content}></MarkdownContent>
      </div>
    </div>
  );
};

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
};

const OutputItem = ({ content, timestamp, logType }: OutputItemProps) => {
  const lineNumbers = content.split("\n").length;
  if (lineNumbers > 1) {
    return <ExpandableOutput content={content} timestamp={timestamp} />;
  }

  return (
    <div className="border-b p-2 border-neutral-200 bg-white flex justify-between items-stretch text-xs">
      <div className="w-10"></div>
      <span className="flex-1 flex justify-between items-center">
        <span className={cx("flex-1", getLogColor(logType))}>{content}</span>
      </span>
      <span className="text-gray-400 text-xs flex justify-end">
        {timestamp}
      </span>
    </div>
  );
};

type MarkdownContentProps = {
  content: string;
};

const MarkdownContent = ({ content }: MarkdownContentProps) => {
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

export default WorkflowOutput;
