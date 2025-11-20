import { LogItem } from "@/services/types";
import Markdown from "@/components/Markdown";
import { ChevronDown, ChevronRight, Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { useCopyTimeout } from "./useCopyTimeout";
import { useCallback } from "react";

type OutputItemProps = {
  onArtifactClick?: (id: string) => void;
  log: LogItem;
  isPending?: boolean;
  depth?: number;
  isExpandable?: boolean;
  isExpanded?: boolean;
  onToggleExpanded?: () => void;
};

const getAllChildrenContent = (item: LogItem): string => {
  let content = "";
  if (!item.children || item.children.length === 0) {
    return item.content;
  }
  item.children.forEach((child) => {
    content += getAllChildrenContent(child) + "\n\n";
  });
  return content.trim();
};

const OutputItem = ({
  depth = 0,
  log,
  onArtifactClick,
  isExpandable = false,
  isExpanded = false,
  onToggleExpanded,
}: OutputItemProps) => {
  const { copied, handleCopy: copyToClipboard } = useCopyTimeout();

  const handleCopy = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      // For expandable items, copy all children content
      const contentToCopy = isExpandable
        ? getAllChildrenContent(log)
        : log.content;
      await copyToClipboard(contentToCopy);
    },
    [isExpandable, log, copyToClipboard],
  );

  if (isExpandable && onToggleExpanded) {
    return (
      <div
        className="w-full min-w-[500px] group"
        style={{ paddingLeft: depth > 0 ? `${depth * 24}px` : undefined }}
      >
        <div
          className="w-full flex items-center justify-center py-2 gap-2 cursor-pointer hover:bg-accent/50 rounded"
          onClick={onToggleExpanded}
        >
          {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}

          <div className="flex-1 text-sm flex justify-between items-center">
            <span>{log.content}</span>
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  "h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity",
                  copied && "opacity-100",
                )}
                onClick={handleCopy}
                title="Copy all results from this step"
              >
                {copied ? (
                  <Check size={14} className="text-green-500" />
                ) : (
                  <Copy size={14} />
                )}
              </Button>
              <span className="text-gray-400 text-xs flex justify-end">
                {log.timestamp}
              </span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="min-w-[500px] group relative"
      style={{
        paddingLeft: depth > 0 ? `${depth * 24}px` : undefined,
      }}
    >
      <div className="flex items-start gap-2">
        <div className="flex-1">
          <Markdown onArtifactClick={onArtifactClick}>{log.content}</Markdown>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            "h-6 w-6 p-0 mt-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0",
            copied && "opacity-100",
          )}
          onClick={handleCopy}
          title="Copy output"
        >
          {copied ? (
            <Check size={14} className="text-green-500" />
          ) : (
            <Copy size={14} />
          )}
        </Button>
      </div>
    </div>
  );
};

export default OutputItem;
