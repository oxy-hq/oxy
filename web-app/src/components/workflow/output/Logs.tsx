import { useVirtualizer } from "@tanstack/react-virtual";
import { LoaderIcon } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useState } from "react";
import Markdown from "@/components/Markdown";
import { cn } from "@/libs/shadcn/utils";
import type { LogItem } from "@/services/types";
import OutputItem from "./Item";

interface FlattenedLogItem {
  id: string;
  log: LogItem;
  depth: number;
  isExpandable: boolean;
  parentId?: string;
  isLastRootItem: boolean;
}

interface OutputLogsProps {
  isPending: boolean;
  logs: LogItem[];
  contentClassName?: string;
  onlyShowResult?: boolean;
  onArtifactClick?: (id: string) => void;
  expandAll?: number;
  collapseAll?: number;
}

const OutputLogs: React.FC<OutputLogsProps> = ({
  isPending,
  logs,
  contentClassName,
  onlyShowResult,
  onArtifactClick,
  expandAll,
  collapseAll
}) => {
  const parentRef = React.useRef<HTMLDivElement | null>(null);
  const bottomRef = React.useRef<HTMLDivElement | null>(null);
  const [itemStates, setItemStates] = useState<Map<string, boolean>>(new Map());

  const flattenedLogs = useMemo(() => {
    const flattened: FlattenedLogItem[] = [];

    const flattenRecursive = (items: LogItem[], depth: number = 0, parentId?: string) => {
      items.forEach((log, index) => {
        const id = parentId ? `${parentId}-${index}` : `root-${index}`;
        const isExpandable = !!(log.children && log.children.length > 0);
        const isLastRootItem = depth === 0 && index === items.length - 1;

        flattened.push({
          id,
          log,
          depth,
          isExpandable,
          parentId,
          isLastRootItem
        });

        const itemState = itemStates.get(id);
        const shouldExpand = itemState === true || (itemState === undefined && isLastRootItem);

        if (isExpandable && shouldExpand) {
          flattenRecursive(log.children!, depth + 1, id);
        }
      });
    };

    flattenRecursive(logs);
    return flattened;
  }, [logs, itemStates]);

  // Handle expand all
  React.useEffect(() => {
    if (expandAll && expandAll > 0) {
      const allExpandableIds: string[] = [];
      const collectIds = (items: LogItem[], parentId?: string) => {
        items.forEach((log, index) => {
          const id = parentId ? `${parentId}-${index}` : `root-${index}`;
          if (log.children && log.children.length > 0) {
            allExpandableIds.push(id);
            collectIds(log.children, id);
          }
        });
      };
      collectIds(logs);
      setItemStates(new Map(allExpandableIds.map((id) => [id, true])));
    }
  }, [expandAll, logs]);

  // Handle collapse all
  React.useEffect(() => {
    if (collapseAll && collapseAll > 0) {
      setItemStates(new Map());
    }
  }, [collapseAll]);

  const toggleExpanded = useCallback((id: string, isLastRootItem: boolean) => {
    setItemStates((prev) => {
      const next = new Map(prev);
      const currentState = next.get(id);

      const isCurrentlyExpanded =
        currentState === true || (currentState === undefined && isLastRootItem);

      next.set(id, !isCurrentlyExpanded);
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
    [flattenedLogs]
  );
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  const getScrollElement = useCallback(() => parentRef.current, [parentRef]);

  // eslint-disable-next-line react-hooks/incompatible-library
  const logsVirtualizer = useVirtualizer({
    count: flattenedLogs.length,
    getScrollElement,
    estimateSize,
    enabled: true
  });

  // Auto-scroll to bottom only while pending and user hasn't scrolled up
  const isUserScrolledUp = React.useRef(false);

  const handleScroll = useCallback(() => {
    const el = parentRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    isUserScrolledUp.current = distanceFromBottom > 100;
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    if (isPending && !isUserScrolledUp.current) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "start" });
    }
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
      onScroll={handleScroll}
      className='customScrollbar scrollbar-gutter-auto relative h-full overflow-y-auto break-all bg-card p-4 pt-0 contain-strict'
      data-testid='workflow-output-logs'
    >
      {!onlyShowResult && (
        <div
          className={cn("relative w-full", contentClassName)}
          style={{ height: logsVirtualizer.getTotalSize() }}
        >
          <div
            className='absolute top-0 left-0 w-full'
            style={{
              transform: `translateY(${items[0]?.start ?? 0}px)`,
              paddingBottom: 100
            }}
          >
            {items.map((virtualItem) => {
              const flatItem = flattenedLogs[virtualItem.index];
              if (!flatItem) return null;

              const itemState = itemStates.get(flatItem.id);
              const isExpanded =
                itemState === true || (itemState === undefined && flatItem.isLastRootItem);

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
                    onToggleExpanded={() => toggleExpanded(flatItem.id, flatItem.isLastRootItem)}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}

      {onlyShowResult && (
        <Markdown onArtifactClick={onArtifactClick}>{lastedContent || ""}</Markdown>
      )}

      {isPending && (
        <div className='flex justify-center p-6'>
          <LoaderIcon className='animate-spin' />
        </div>
      )}
      <div ref={bottomRef} />
    </div>
  );
};

export default OutputLogs;
