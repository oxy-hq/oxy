import { Check, ChevronDown, ChevronRight, Copy } from "lucide-react";
import { useCallback } from "react";
import Markdown from "@/components/Markdown";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import type { LogItem } from "@/services/types";
import { useCopyTimeout } from "./useCopyTimeout";

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
    content += `${getAllChildrenContent(child)}\n\n`;
  });
  return content.trim();
};

const OutputItem = ({
  depth = 0,
  log,
  onArtifactClick,
  isExpandable = false,
  isExpanded = false,
  onToggleExpanded
}: OutputItemProps) => {
  const { copied, handleCopy: copyToClipboard } = useCopyTimeout();

  const handleCopy = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      // For expandable items, copy all children content
      const contentToCopy = isExpandable ? getAllChildrenContent(log) : log.content;
      await copyToClipboard(contentToCopy);
    },
    [isExpandable, log, copyToClipboard]
  );

  if (isExpandable && onToggleExpanded) {
    return (
      <div
        className='group w-full min-w-[500px]'
        style={{ paddingLeft: depth > 0 ? `${depth * 24}px` : undefined }}
        data-testid='workflow-output-item'
      >
        <div
          className='flex w-full cursor-pointer items-center justify-center gap-2 rounded py-2 hover:bg-accent/50'
          onClick={onToggleExpanded}
        >
          {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}

          <div className='flex flex-1 items-center justify-between text-sm'>
            <span>{log.content}</span>
            <div className='flex items-center gap-2'>
              <Button
                variant='ghost'
                size='sm'
                className={cn(
                  "h-6 w-6 p-0 opacity-0 transition-opacity group-hover:opacity-100",
                  copied && "opacity-100"
                )}
                onClick={handleCopy}
                title='Copy all results from this step'
              >
                {copied ? <Check size={14} className='text-green-500' /> : <Copy size={14} />}
              </Button>
              <span className='flex justify-end text-gray-400 text-xs'>{log.timestamp}</span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className='group relative min-w-[500px]'
      style={{
        paddingLeft: depth > 0 ? `${depth * 24}px` : undefined
      }}
      data-testid='workflow-output-item'
    >
      <div className='flex items-start gap-2'>
        <div className='flex-1'>
          <Markdown onArtifactClick={onArtifactClick}>{log.content}</Markdown>
        </div>
        <Button
          variant='ghost'
          size='sm'
          className={cn(
            "mt-1 h-6 w-6 flex-shrink-0 p-0 opacity-0 transition-opacity group-hover:opacity-100",
            copied && "opacity-100"
          )}
          onClick={handleCopy}
          title='Copy output'
        >
          {copied ? <Check size={14} className='text-green-500' /> : <Copy size={14} />}
        </Button>
      </div>
    </div>
  );
};

export default OutputItem;
