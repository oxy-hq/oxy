import React from "react";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import { Separator } from "@/components/ui/shadcn/separator";
import AnswerContent from "@/components/AnswerContent";
import { ThreadItem } from "@/types/chat";

interface ThreadListProps {
  threads: ThreadItem[];
}

export type { ThreadListProps };

const ThreadListItem = ({ thread }: { thread: ThreadItem }) => {
  return (
    <div className="flex flex-col gap-6">
      <Link to={`/threads/${thread.id}`} className="cursor-pointer group">
        <div className="flex flex-col gap-4">
          <Badge variant="secondary">{thread.source}</Badge>
          <div className="flex flex-col gap-2">
            <h2 className="text-xl font-medium group-hover:text-accent-main-000">
              {thread.title}
            </h2>
            <div className="text-sm max-h-[100px] overflow-hidden relative after:absolute after:bottom-0 after:left-0 after:w-full after:h-8 after:bg-gradient-to-t after:from-background">
              <AnswerContent className="text-sm" content={thread.output} />
            </div>
          </div>
        </div>
      </Link>
      <Separator orientation="horizontal" />
    </div>
  );
};

const ThreadList: React.FC<ThreadListProps> = ({ threads }) => {
  return (
    <div className="flex flex-col gap-6">
      {threads.map((thread) => (
        <ThreadListItem key={thread.id} thread={thread} />
      ))}
    </div>
  );
};

export default ThreadList;
