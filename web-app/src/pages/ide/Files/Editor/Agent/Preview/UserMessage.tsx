import { Message } from "@/types/chat";

const UserMessage = ({ message }: { message: Message }) => {
  const { content } = message;

  return (
    <div className="flex gap-2 justify-end items-start">
      <div className="flex flex-col gap-1 max-w-[80%]">
        <div className="bg-primary text-primary-foreground rounded-xl px-4 py-2">
          <p>{content}</p>
        </div>
      </div>
      <div className="w-8 h-8 rounded-full bg-primary flex items-center justify-center">
        <p className="text-primary-foreground text-sm">U</p>
      </div>
    </div>
  );
};

export default UserMessage;
