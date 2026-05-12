import { Bot } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useAgent from "@/hooks/api/agents/useAgent";
import { encodeBase64 } from "@/libs/encoding";
import type { ThreadItem } from "@/types/chat";

interface ThreadHeaderProps {
  thread: ThreadItem;
}

const ThreadHeader = ({ thread }: ThreadHeaderProps) => {
  const agentPath64 = encodeBase64(thread.source);
  const { data: agent, isPending } = useAgent(agentPath64);

  const agentName = agent?.name || thread.source;

  return (
    <PageHeader className='items-center border-border border-b-1'>
      <div className='flex h-full min-w-0 flex-1 flex-col items-stretch gap-1 p-2 md:flex-row md:items-center md:justify-center md:gap-0'>
        <div className='flex min-w-0 items-center gap-1 text-muted-foreground md:flex-1 md:justify-end'>
          <Bot className='h-4 min-h-4 w-4 min-w-4' />
          <div className='truncate text-sm md:break-all'>
            {isPending ? <Skeleton className='h-[16px] w-[80px] rounded-full' /> : agentName}
          </div>
        </div>
        <div className='hidden h-full items-stretch px-4 md:flex'>
          <Separator orientation='vertical' />
        </div>
        <p className='min-w-0 truncate text-base-foreground text-sm md:flex-1'>{thread?.title}</p>
      </div>
    </PageHeader>
  );
};

export default ThreadHeader;
