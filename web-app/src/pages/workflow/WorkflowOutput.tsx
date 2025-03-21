import React, { useCallback, useEffect } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import directive from "remark-directive";
import CodeBlock from "@/components/CodeBlock";
import { DynamicIcon } from "lucide-react/dynamic";
import { LoaderIcon } from "lucide-react";
import { LogItem, LogType } from "@/hooks/api/runWorkflow";
import { Button } from "@/components/ui/shadcn/button";
import { useVirtualizer } from "@tanstack/react-virtual";
import dayjs from "dayjs";
import { cx } from "class-variance-authority";

interface WorkflowOutputProps {
  showOutput: boolean;
  toggleOutput: () => void;
  isPending: boolean;
  logs: LogItem[];
}

const WorkflowOutput: React.FC<WorkflowOutputProps> = ({
  showOutput,
  toggleOutput,
  isPending,
  logs,
}) => {
  const parentRef = React.useRef<HTMLDivElement | null>(null);
  const estimateSize = (index: number) => {
    const log = logs[index];
    const lineNumbers = log.content
      .split("\n\n")
      .map((line) => line.split("\n").length)
      .reduce((a, b) => a + b, 0);
    if (lineNumbers > 1) {
      return 20 * lineNumbers + 20;
    }
    return 33;
  };
  const logsVirtualizer = useVirtualizer({
    count: logs.length,
    getScrollElement: () => parentRef.current,
    overscan: 20,
    estimateSize: estimateSize,
    enabled: true,
  });
  const scrollToBottom = useCallback(() => {
    logsVirtualizer.scrollToIndex(logs.length - 1, {
      // smooth behavior is not currently working properly for dynamic sized list
      // behavior: "smooth",
      align: "start",
    });
  }, [logsVirtualizer, logs]);
  useEffect(() => {
    scrollToBottom();
    // sometimes the virtualizer takes a while to calculate the size
    // so we need to scroll multiple times just in case
    requestAnimationFrame(() => {
      scrollToBottom();
      requestAnimationFrame(() => {
        scrollToBottom();
      });
    });
  }, [logs, logsVirtualizer, scrollToBottom]);
  const items = logsVirtualizer.getVirtualItems();

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
            ref={parentRef}
            className="h-75 relative overflow-y-auto scrollbar-thin break-all p-1 contain-strict"
          >
            <div
              style={{
                height: logsVirtualizer.getTotalSize(),
                width: "100%",
                position: "relative",
              }}
            >
              <div
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${items[0]?.start ?? 0}px)`,
                }}
              >
                {items.map((item) => {
                  const log = logs[item.index];
                  return (
                    <div
                      key={item.index}
                      data-index={item.index}
                      ref={logsVirtualizer.measureElement}
                    >
                      <OutputItem
                        content={log.content}
                        timestamp={dayjs(log.timestamp).format(
                          "ddd YYYY-MM-DD H:mm:ss",
                        )}
                        logType={log.log_type}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
            {isPending && (
              <div className="p-2 flex justify-center">
                <LoaderIcon className="animate-spin" />
              </div>
            )}
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

export default WorkflowOutput;
