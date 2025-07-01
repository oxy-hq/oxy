import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useAgent from "@/hooks/api/useAgent";
import { ThreadItem } from "@/types/chat";
import { Bot } from "lucide-react";

interface ThreadHeaderProps {
  thread: ThreadItem;
}

const ThreadHeader = ({ thread }: ThreadHeaderProps) => {
  const agentPath64 = btoa(thread.source);
  const { data: agent, isPending } = useAgent(agentPath64);

  const agentName = agent?.name || thread.source;

  return (
    <PageHeader className="border-b-1 border-border items-center">
      <div className="p-2 flex items-center justify-center flex-1 h-full">
        <div className="flex flex-1 gap-1 items-center text-muted-foreground justify-end">
          <Bot className="w-4 h-4 min-w-4 min-h-4" />
          <div className="text-sm break-all">
            {isPending ? (
              <Skeleton className="w-[80px] h-[16px] rounded-full" />
            ) : (
              agentName
            )}
          </div>
        </div>
        <div className="px-4 h-full flex items-stretch">
          <Separator orientation="vertical" />
        </div>
        <p className="text-sm text-base-foreground flex-1">{thread?.title}</p>
      </div>
    </PageHeader>
  );
};

export default ThreadHeader;
