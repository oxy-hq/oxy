import type React from "react";
import { useNavigate } from "react-router-dom";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/shadcn/badge";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import { timeAgo } from "@/libs/utils/date";
import ROUTES from "@/libs/utils/routes";
import type { ThreadItem } from "@/types/chat";

interface ThreadListProps {
  threads: ThreadItem[];
  selectedThreads: Set<string>;
  onThreadSelect: (threadId: string, selected: boolean) => void;
  isSelectionMode: boolean;
}

export type { ThreadListProps };

interface ThreadListItemProps {
  thread: ThreadItem;
  isSelected: boolean;
  onSelect: (threadId: string, selected: boolean) => void;
  isSelectionMode?: boolean;
}

const ThreadListItem = ({
  thread,
  isSelected = false,
  onSelect,
  isSelectionMode = false
}: ThreadListItemProps) => {
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const handleCheckboxChange = (checked: boolean) => {
    onSelect(thread.id, checked);
  };

  const handleItemClick = (e: React.MouseEvent) => {
    if (isSelectionMode) {
      e.preventDefault();
      handleCheckboxChange(!isSelected);
      return;
    }
    const threadUri = ROUTES.PROJECT(project.id).THREAD(thread.id);
    navigate(threadUri);
  };

  return (
    <div
      data-testid='thread-item'
      className={cn(
        "relative flex gap-4 rounded-lg border p-4",
        "group cursor-pointer hover:border-accent-main-000"
      )}
      onClick={handleItemClick}
    >
      <Checkbox
        className={cn(
          "bg-muted opacity-0 transition-opacity group-hover:opacity-100",
          "absolute top-1/2 left-0 -translate-x-1/2 -translate-y-1/2",
          isSelectionMode && "opacity-100"
        )}
        checked={isSelected}
        onCheckedChange={handleCheckboxChange}
        onClick={(e) => e.stopPropagation()}
      />
      <div className='flex flex-col gap-4'>
        <Badge variant='secondary' data-testid='thread-agent-type'>
          {thread.source}
        </Badge>
        <div className='flex flex-col gap-2'>
          <h2 className='font-medium text-xl' data-testid='thread-title'>
            {thread.title}
          </h2>
          <p className='text-muted-foreground text-xs' data-testid='thread-timestamp'>
            {timeAgo(thread.created_at)}
          </p>
        </div>
      </div>
    </div>
  );
};

const ThreadList: React.FC<ThreadListProps> = ({
  threads,
  selectedThreads = new Set(),
  isSelectionMode,
  onThreadSelect
}) => {
  return (
    <div className='flex flex-col gap-6'>
      {threads.map((thread) => (
        <ThreadListItem
          key={thread.id}
          thread={thread}
          isSelected={selectedThreads.has(thread.id)}
          onSelect={onThreadSelect}
          isSelectionMode={isSelectionMode}
        />
      ))}
    </div>
  );
};

export default ThreadList;
