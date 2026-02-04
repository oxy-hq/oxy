import { MessageSquare } from "lucide-react";

const EmptyThreads = () => {
  return (
    <div className='flex flex-col items-center justify-center gap-6 p-6'>
      <div className='flex h-[48px] w-[48px] items-center justify-center rounded-md border border-border p-2 shadow-sm'>
        <MessageSquare />
      </div>
      <div className='flex flex-col items-center gap-2'>
        <p className='font-semibold text-xl'>No threads</p>
        <p className='text-muted-foreground text-sm'>
          Start by asking an agent of your choice a question
        </p>
      </div>
    </div>
  );
};

export default EmptyThreads;
