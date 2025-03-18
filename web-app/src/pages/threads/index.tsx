import useThreads from "@/hooks/api/useThreads";
import { MessageSquare, MessagesSquare } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Separator } from "@/components/ui/shadcn/separator";
import { Link } from "react-router-dom";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { ThreadItem } from "@/types/chat";
import { Button } from "@/components/ui/shadcn/button";
import AnswerContent from "@/components/AnswerContent";
import PageHeader from "@/components/PageHeader";

const Threads = () => {
  const { data: threads, isLoading } = useThreads();

  return (
    <div className="py-4 gap-8 flex flex-col h-full">
      <PageHeader className="flex-col border-b border-border max-w-[742px] w-full mx-auto">
        <div className="px-2 flex gap-[10px] items-center pt-8">
          <MessagesSquare className="w-9 h-9 min-w-9 min-h-9" strokeWidth={1} />
          <h1 className="text-3xl font-semibold">Threads</h1>
        </div>
      </PageHeader>

      <div className="overflow-y-auto customScrollbar">
        <div className="max-w-[742px] px-4 w-full mx-auto flex flex-col gap-6">
          {isLoading && <ThreadsSkeleton />}
          {!isLoading && threads && threads.length > 0 && (
            <ThreadList threads={threads} />
          )}
          {!isLoading && (!threads || threads.length === 0) && <EmptyThreads />}
        </div>
      </div>
    </div>
  );
};

const ThreadsSkeleton = () => {
  return (
    <div className="flex flex-col gap-10">
      {Array.from({ length: 3 }).map((_, index) => (
        <div key={index} className="flex flex-col gap-4">
          <Skeleton className="h-4 max-w-[200px]" />
          <Skeleton className="h-4 max-w-[500px]" />
          <Skeleton className="h-4 max-w-[500px]" />
        </div>
      ))}
    </div>
  );
};

const EmptyThreads = () => {
  return (
    <div className="flex flex-col gap-6 p-6 items-center justify-center">
      <div className="w-[48px] h-[48px] flex p-2 rounded-md border border-border shadow-sm items-center justify-center">
        <MessageSquare />
      </div>
      <div className="flex flex-col gap-2 items-center">
        <p className="text-xl font-semibold">No threads</p>
        <p className="text-sm text-muted-foreground">
          Start by asking an agent of your choice a question
        </p>
      </div>
      <Button variant="outline" asChild>
        <Link to="/">Start a new thread</Link>
      </Button>
    </div>
  );
};

const ThreadList = ({ threads }: { threads: ThreadItem[] }) => {
  return (
    <>
      {threads
        ?.sort(
          (a, b) =>
            new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
        )
        .map((thread) => (
          <div className="flex flex-col gap-6" key={thread.id}>
            <Link
              key={thread.id}
              to={`/threads/${thread.id}`}
              className="cursor-pointer group"
            >
              <div className="flex flex-col gap-4">
                <Badge variant="secondary">{thread.agent}</Badge>
                <div className="flex flex-col gap-2">
                  <h2 className="text-xl font-medium group-hover:text-accent-main-000">
                    {thread.title}
                  </h2>
                  <div className="text-sm max-h-[100px] overflow-hidden relative after:absolute after:bottom-0 after:left-0 after:w-full after:h-8 after:bg-gradient-to-t after:from-background">
                    <AnswerContent
                      className="text-sm"
                      content={thread.answer}
                    />
                  </div>
                </div>
              </div>
            </Link>
            <Separator orientation="horizontal" />
          </div>
        ))}
    </>
  );
};

export default Threads;
