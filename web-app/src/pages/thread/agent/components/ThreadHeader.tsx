import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import { ThreadItem } from "@/types/chat";
import { Bot } from "lucide-react";

interface ThreadHeaderProps {
  thread: ThreadItem;
}

const ThreadHeader = ({ thread }: ThreadHeaderProps) => (
  <PageHeader className="border-b-1 border-border items-center">
    <div className="p-2 flex items-center justify-center flex-1 h-full">
      <div className="flex gap-1 items-center text-muted-foreground">
        <Bot className="w-4 h-4 min-w-4 min-h-4" />
        <p className="text-sm break-all">{thread?.source}</p>
      </div>
      <div className="px-4 h-full flex items-stretch">
        <Separator orientation="vertical" />
      </div>
      <p className="text-sm text-base-foreground">{thread?.title}</p>
    </div>
  </PageHeader>
);

export default ThreadHeader;
