import { Button } from "@/components/ui/shadcn/button";
import { Link, MessageSquare } from "lucide-react";

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

export default EmptyThreads;
