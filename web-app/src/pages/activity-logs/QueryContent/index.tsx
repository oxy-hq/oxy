import { LogItem } from "@/types/logs";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { formatLogContent } from "./utils";
import QueryRow from "./QueryRow";

const QueryContent = ({ log }: { log: LogItem }) => {
  const [showAllQueries, setShowAllQueries] = useState(false);

  if (log.log && typeof log.log === "object" && "queries" in log.log) {
    const logData = log.log as Record<string, unknown>;
    const queries = logData.queries;

    if (Array.isArray(queries) && queries.length > 0) {
      const firstQuery = queries[0];
      const hasMultipleQueries = queries.length > 1;

      return (
        <>
          <div className="space-y-1">
            <QueryRow queryItem={firstQuery} />

            {hasMultipleQueries && (
              <div className="flex items-center justify-between">
                <p className="text-sm text-muted-foreground">
                  Total {queries.length} queries
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setShowAllQueries(true)}
                >
                  Show all
                </Button>
              </div>
            )}
          </div>

          <Dialog open={showAllQueries} onOpenChange={setShowAllQueries}>
            <DialogContent className="max-w-4xl! max-h-[80vh] overflow-hidden flex flex-col p-0 gap-0">
              <DialogHeader className="p-4">
                <DialogTitle>All Queries ({queries.length})</DialogTitle>
                <DialogDescription>
                  Complete list of queries from this log entry
                </DialogDescription>
              </DialogHeader>
              <div className="flex-1 overflow-y-auto customScrollbar space-y-4 p-4">
                {queries.map(
                  (queryItem: Record<string, unknown>, index: number) => (
                    <QueryRow key={index} queryItem={queryItem} />
                  ),
                )}
              </div>
            </DialogContent>
          </Dialog>
        </>
      );
    }
  }

  return (
    <div className="text-sm text-muted-foreground font-mono whitespace-pre-line">
      {formatLogContent(log.log)}
    </div>
  );
};

export default QueryContent;
