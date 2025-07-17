import React from "react";
import { Link } from "react-router-dom";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { formatDate } from "@/libs/utils/date";
import { LogItem } from "@/types/logs";
import LogInfo from "./LogInfo";
import useSettingsPage from "@/stores/useSettingsPage";

interface Props {
  log: LogItem;
}

const LogRow: React.FC<Props> = ({ log }) => {
  const { setIsOpen: setIsSettingsOpen } = useSettingsPage();
  const [open, setOpen] = React.useState(false);
  const getFirstQuery = () => {
    if (!log.log?.queries || log.log.queries.length === 0) {
      return "No queries";
    }

    return log.log.queries[0]?.query || "No query content";
  };

  return (
    <>
      <TableRow
        key={log.id}
        onClick={() => setOpen(true)}
        className="cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
      >
        <TableCell className="whitespace-pre-wrap break-words">
          <Link
            to={`/threads/${log.thread_id}`}
            onClick={(e) => {
              e.stopPropagation();
              setIsSettingsOpen(false);
            }}
            className="text-blue-600 dark:text-blue-400 hover:underline"
          >
            {log.thread?.title || "Untitled Thread"}
          </Link>
        </TableCell>
        <TableCell className="whitespace-pre-wrap break-words">
          {log.prompts || "No prompt provided"}
        </TableCell>
        <TableCell className="max-w-[300px] truncate font-mono text-sm">
          {getFirstQuery()}
        </TableCell>
        <TableCell className="w-[170px] whitespace-nowrap break-words">
          {formatDate(log.created_at)}
        </TableCell>
      </TableRow>
      <LogInfo log={log} open={open} onOpenChange={setOpen} />
    </>
  );
};

export default LogRow;
