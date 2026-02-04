import { Bot } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useAgent from "@/hooks/api/agents/useAgent";
import type { ThreadItem } from "@/types/chat";

interface ThreadHeaderProps {
  thread: ThreadItem;
}

const ThreadHeader = ({ thread }: ThreadHeaderProps) => {
  const agentPath64 = btoa(thread.source);
  const { data: agent, isPending } = useAgent(agentPath64);

  const agentName = agent?.name || thread.source;

  return (
    <PageHeader className='items-center border-border border-b-1'>
      <div className='flex h-full flex-1 items-center justify-center p-2'>
        <div className='flex flex-1 items-center justify-end gap-1 text-muted-foreground'>
          <Bot className='h-4 min-h-4 w-4 min-w-4' />
          <div className='break-all text-sm'>
            {isPending ? <Skeleton className='h-[16px] w-[80px] rounded-full' /> : agentName}
          </div>
        </div>
        <div className='flex h-full items-stretch px-4'>
          <Separator orientation='vertical' />
        </div>
        <p className='flex-1 text-base-foreground text-sm'>{thread?.title}</p>
      </div>
    </PageHeader>
  );
};

export default ThreadHeader;
