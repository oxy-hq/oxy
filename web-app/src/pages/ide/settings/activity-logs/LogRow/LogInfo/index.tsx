import { LogItem } from "@/types/logs";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { Link } from "react-router-dom";
import Query from "./Query";
import { formatDate } from "@/libs/utils/date";
import { Label } from "@/components/ui/shadcn/label";

interface Props {
  log: LogItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const LogInfo = ({ log, open, onOpenChange }: Props) => {
  const queries = log.log?.queries || [];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl! max-h-[80vh] overflow-hidden flex flex-col p-0 gap-0">
        <DialogHeader className="p-4 gap-1">
          <DialogTitle>
            <Link
              to={`/threads/${log.thread_id}`}
              onClick={(e) => {
                e.stopPropagation();
                onOpenChange(false);
              }}
              className="text-blue-600 dark:text-blue-400 hover:underline"
            >
              {log.thread?.title || "Untitled Thread"}
            </Link>
          </DialogTitle>
          <DialogDescription>{formatDate(log.created_at)}</DialogDescription>
        </DialogHeader>
        <div className="flex-1 overflow-y-auto customScrollbar space-y-4 p-4">
          <div>
            <Label className="text-sm font-medium">Prompts</Label>
            <p className="text-sm text-muted-foreground">
              {log.prompts || "No prompt provided"}
            </p>
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
