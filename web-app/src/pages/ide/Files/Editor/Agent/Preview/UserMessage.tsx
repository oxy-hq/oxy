import type { Message } from "@/types/chat";

const UserMessage = ({ message }: { message: Message }) => {
  const { content } = message;

  return (
    <div className='flex items-start justify-end gap-2'>
      <div className='flex max-w-[80%] flex-col gap-1'>
        <div className='rounded-xl bg-primary px-4 py-2 text-primary-foreground'>
          <p>{content}</p>
        </div>
      </div>
      <div className='flex h-8 w-8 items-center justify-center rounded-full bg-primary'>
        <p className='text-primary-foreground text-sm'>U</p>
      </div>
    </div>
  );
};

export default UserMessage;
