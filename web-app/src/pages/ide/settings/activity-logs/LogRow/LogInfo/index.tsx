import { Link } from "react-router-dom";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Label } from "@/components/ui/shadcn/label";
import { formatDate } from "@/libs/utils/date";
import type { LogItem } from "@/types/logs";
import Query from "./Query";

interface Props {
  log: LogItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const LogInfo = ({ log, open, onOpenChange }: Props) => {
  const queries = log.log?.queries || [];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='flex max-h-[80vh] max-w-4xl! flex-col gap-0 overflow-hidden p-0'>
        <DialogHeader className='gap-1 p-4'>
          <DialogTitle>
            <Link
              to={`/threads/${log.thread_id}`}
              onClick={(e) => {
                e.stopPropagation();
                onOpenChange(false);
              }}
              className='text-blue-600 hover:underline dark:text-blue-400'
            >
              {log.thread?.title || "Untitled Thread"}
            </Link>
          </DialogTitle>
          <DialogDescription>{formatDate(log.created_at)}</DialogDescription>
        </DialogHeader>
        <div className='customScrollbar flex-1 space-y-4 overflow-y-auto p-4'>
          <div>
            <Label className='font-medium text-sm'>Prompts</Label>
            <p className='text-muted-foreground text-sm'>{log.prompts || "No prompt provided"}</p>
          </div>

          {queries.map((queryItem, index: number) => (
            <Query key={index} queryItem={queryItem} />
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default LogInfo;
