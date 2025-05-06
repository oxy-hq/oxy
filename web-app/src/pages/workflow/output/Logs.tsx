import React, { useCallback, useEffect } from "react";
import { LoaderIcon } from "lucide-react";
import { LogItem } from "@/hooks/api/runWorkflow";
import { useVirtualizer } from "@tanstack/react-virtual";
import dayjs from "dayjs";
import OutputItem from "./Item";

interface OutputLogsProps {
  isPending: boolean;
  logs: LogItem[];
}

const OutputLogs: React.FC<OutputLogsProps> = ({ isPending, logs }) => {
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
    <div
      ref={parentRef}
      className="h-full relative overflow-y-auto customScrollbar break-all contain-strict"
    >
      <div
        className="relative w-full"
        style={{ height: logsVirtualizer.getTotalSize() }}
      >
        <div
          className="absolute top-0 left-0 w-full"
          style={{
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
  );
};

export default OutputLogs;
