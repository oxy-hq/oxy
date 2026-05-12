import { BarChart2, Hammer } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import type { ThreadItem } from "@/types/chat";

const Header = ({ thread }: { thread: ThreadItem }) => {
  const isBuilder = thread.source === "__builder__";
  return (
    <PageHeader className='items-center border-border border-b-1'>
      <div className='flex h-full min-w-0 flex-1 flex-col items-stretch gap-1 p-2 md:flex-row md:items-center md:justify-center md:gap-0'>
        <div className='flex min-w-0 items-center gap-1 text-muted-foreground md:flex-1 md:justify-end'>
          {isBuilder ? (
            <Hammer className='h-4 min-h-4 w-4 min-w-4' />
          ) : (
            <BarChart2 className='h-4 min-h-4 w-4 min-w-4' />
          )}
          <p className='truncate text-sm md:break-all'>{isBuilder ? "Builder" : "Analytics"}</p>
        </div>
        <div className='hidden h-full items-stretch px-4 md:flex'>
          <Separator orientation='vertical' />
        </div>
        <p className='min-w-0 truncate text-base-foreground text-sm md:flex-1'>{thread?.title}</p>
      </div>
    </PageHeader>
  );
};

export default Header;
