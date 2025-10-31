import React, { useCallback, useEffect, useMemo, useState } from "react";
import { LoaderIcon } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import OutputItem from "./Item";
import { cn } from "@/libs/shadcn/utils";
import { LogItem } from "@/services/types";
import Markdown from "@/components/Markdown";

interface FlattenedLogItem {
  id: string;
  log: LogItem;
  depth: number;
  isExpandable: boolean;
  parentId?: string;
}

interface OutputLogsProps {
  isPending: boolean;
  logs: LogItem[];
  contentClassName?: string;
  onlyShowResult?: boolean;
  onArtifactClick?: (id: string) => void;
}

const OutputLogs: React.FC<OutputLogsProps> = ({
  isPending,
  logs,
  contentClassName,
  onlyShowResult,
  onArtifactClick,
}) => {
  const parentRef = React.useRef<HTMLDivElement | null>(null);
  const bottomRef = React.useRef<HTMLDivElement | null>(null);
  const [expandedItems, setExpandedItems] = useState<Set<string>>(new Set());

  const flattenedLogs = useMemo(() => {
    const flattened: FlattenedLogItem[] = [];

    const flattenRecursive = (
      items: LogItem[],
      depth: number = 0,
      parentId?: string,
    ) => {
      items.forEach((log, index) => {
        const id = parentId ? `${parentId}-${index}` : `root-${index}`;
        const isExpandable = !!(log.children && log.children.length > 0);

        flattened.push({
          id,
          log,
          depth,
          isExpandable,
          parentId,
        });
        if (
          isExpandable &&
          (expandedItems.has(id) || (isPending && index === items.length - 1))
        ) {
          flattenRecursive(log.children!, depth + 1, id);
        }
      });
    };

    flattenRecursive(logs);
    return flattened;
  }, [logs, expandedItems, isPending]);

  const toggleExpanded = useCallback((id: string) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const estimateSize = useCallback(
    (index: number) => {
      const item = flattenedLogs[index];
      if (!item) return 33;

      const log = item.log;
      const lineNumbers = (log.content || "")
        .split("\n\n")
        .map((line) => line.split("\n").length)
        .reduce((a, b) => a + b, 0);
      if (lineNumbers > 1) {
        return 20 * lineNumbers + 20;
      }
      return 33;
    },
    [flattenedLogs],
  );
  const getScrollElement = useCallback(() => parentRef.current, [parentRef]);

  const logsVirtualizer = useVirtualizer({
    count: flattenedLogs.length,
    getScrollElement,
    estimateSize,
    enabled: true,
  });

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const items = logsVirtualizer.getVirtualItems();

  const lastedContent = useMemo(() => {
    if (flattenedLogs.length === 0) return null;

    const lastItem = flattenedLogs[flattenedLogs.length - 1];

    const getDeepestContent = (logItem: LogItem): string => {
      if (logItem.children && logItem.children.length > 0) {
        const lastChild = logItem.children[logItem.children.length - 1];
        return getDeepestContent(lastChild);
      }

      return logItem.content;
    };

    return getDeepestContent(lastItem.log);
  }, [flattenedLogs]);

  return (
    <div
      ref={parentRef}
      className="h-full relative overflow-y-auto customScrollbar break-all contain-strict scrollbar-gutter-auto bg-card p-4 pt-0"
    >
      {!onlyShowResult && (
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
            {items.map((virtualItem) => {
              const flatItem = flattenedLogs[virtualItem.index];
              if (!flatItem) return null;

              const isLasted = virtualItem.index === flattenedLogs.length - 1;
              const isExpanded = expandedItems.has(flatItem.id);

              return (
                <div
                  key={virtualItem.key}
                  data-index={virtualItem.index}
                  ref={logsVirtualizer.measureElement}
                >
                  <OutputItem
                    isPending={isPending}
                    onArtifactClick={onArtifactClick}
                    log={flatItem.log}
                    depth={flatItem.depth}
                    isExpandable={flatItem.isExpandable}
                    isExpanded={isExpanded}
                    onToggleExpanded={() => toggleExpanded(flatItem.id)}
                    isLasted={isLasted}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}

      {onlyShowResult && (
        <Markdown onArtifactClick={onArtifactClick}>
          {lastedContent || ""}
        </Markdown>
      )}

      {isPending && (
        <div className="p-6 flex justify-center">
          <LoaderIcon className="animate-spin" />
        </div>
      )}
      <div ref={bottomRef} />
    </div>
  );
};

export default OutputLogs;
