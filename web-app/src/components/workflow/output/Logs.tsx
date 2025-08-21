import React, { useCallback, useEffect } from "react";
import { LoaderIcon } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import dayjs from "dayjs";
import OutputItem from "./Item";
import { cn } from "@/libs/shadcn/utils";
import { LogItem } from "@/services/types";

interface OutputLogsProps {
  isPending: boolean;
  logs: LogItem[];
  contentClassName?: string;
  onArtifactClick?: (id: string) => void;
}

const OutputLogs: React.FC<OutputLogsProps> = ({
  isPending,
  logs,
  contentClassName,
  onArtifactClick,
}) => {
  const parentRef = React.useRef<HTMLDivElement | null>(null);

  const estimateSize = useCallback(
    (index: number) => {
      const log = logs[index];
      const lineNumbers = (log.content || "")
        .split("\n\n")
        .map((line) => line.split("\n").length)
        .reduce((a, b) => a + b, 0);
      if (lineNumbers > 1) {
        return 20 * lineNumbers + 20;
      }
      return 33;
    },
    [logs],
  );
  const getScrollElement = useCallback(() => parentRef.current, [parentRef]);

  const logsVirtualizer = useVirtualizer({
    count: logs.length,
    getScrollElement,
    estimateSize,
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
    requestAnimationFrame(() => {
      scrollToBottom();
      requestAnimationFrame(() => {
        scrollToBottom();
      });
    });
  }, [logs, scrollToBottom, logsVirtualizer]);

  const items = logsVirtualizer.getVirtualItems();

  return (
    <div
      ref={parentRef}
      className="h-full relative overflow-y-auto customScrollbar break-all contain-strict"
    >
      <div
        className={cn("relative w-full", contentClassName)}
        style={{ height: logsVirtualizer.getTotalSize() }}
      >
        <div
          className="absolute top-0 left-0 w-full"
          style={{
            transform: `translateY(${items[0]?.start ?? 0}px)`,
            paddingBottom: 100,
          }}
        >
          {items.map((item) => {
            const log = logs[item.index];
            return (
              <div
                key={item.key}
                data-index={item.index}
                ref={logsVirtualizer.measureElement}
              >
                <OutputItem
                  onArtifactClick={onArtifactClick}
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
  );
};

export default OutputLogs;
