import { FileCheck2 } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import type { ThreadItem } from "@/types/chat";

const Header = ({ thread }: { thread: ThreadItem }) => {
  return (
    <PageHeader className='items-center border-border border-b-1'>
      <div className='flex h-full flex-1 items-center justify-center p-2'>
        <div className='flex items-center gap-1 text-muted-foreground'>
          <FileCheck2 className='h-4 min-h-4 w-4 min-w-4' />
          <p className='break-all text-sm'>Agentic workflow</p>
        </div>
        <div className='flex h-full items-stretch px-4'>
          <Separator orientation='vertical' />
        </div>

        <p className='text-base-foreground text-sm'>{thread?.title}</p>
      </div>
    </PageHeader>
  );
};

export default Header;
