import React from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { ThreadItem } from "@/types/chat";
import { cn } from "@/libs/shadcn/utils";
import { timeAgo } from "@/libs/utils/date";

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
  isSelectionMode = false,
}: ThreadListItemProps) => {
  const navigate = useNavigate();
  const handleCheckboxChange = (checked: boolean) => {
    onSelect(thread.id, checked);
  };

  const handleItemClick = (e: React.MouseEvent) => {
    if (isSelectionMode) {
      e.preventDefault();
      handleCheckboxChange(!isSelected);
      return;
    }
    navigate(`/threads/${thread.id}`);
  };

  return (
    <div
      className={cn(
        "flex gap-4 rounded-lg border p-4 relative",
        "group hover:border-accent-main-000 cursor-pointer",
      )}
      onClick={handleItemClick}
    >
      <Checkbox
        className={cn(
          "bg-muted opacity-0 group-hover:opacity-100 transition-opacity",
          "absolute top-1/2 left-0 -translate-y-1/2 -translate-x-1/2",
          isSelectionMode && "opacity-100",
        )}
        checked={isSelected}
        onCheckedChange={handleCheckboxChange}
        onClick={(e) => e.stopPropagation()}
      />
      <div className="flex flex-col gap-4">
        <Badge variant="secondary">{thread.source}</Badge>
        <div className="flex flex-col gap-2">
          <h2 className="text-xl font-medium ">{thread.title}</h2>
          <p className="text-xs text-muted-foreground">
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
  onThreadSelect,
}) => {
  return (
    <div className="flex flex-col gap-6">
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
