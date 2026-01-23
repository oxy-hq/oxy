import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import {
  RefreshCw,
  MoreHorizontal,
  Table as TableIcon,
  Loader2,
} from "lucide-react";
import { DatabaseInfo } from "@/types/database";

interface Props {
  database: DatabaseInfo;
  isLoading: boolean;
  onSync: (datasets?: string[]) => void;
}

const Actions: React.FC<Props> = ({ database, isLoading, onSync }) => {
  const datasetKeys = Object.keys(database.datasets);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" disabled={isLoading}>
          {isLoading ? (
            <Loader2 className="animate-spin" />
          ) : (
            <MoreHorizontal />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          className="cursor-pointer"
          onClick={() => onSync()}
          disabled={isLoading}
        >
          <RefreshCw />
          Sync all datasets
        </DropdownMenuItem>
        {datasetKeys.map((dataset) => (
          <DropdownMenuItem
            className="cursor-pointer"
            key={dataset}
            onClick={() => onSync([dataset])}
            disabled={isLoading}
          >
            <TableIcon />
            Sync {dataset}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default Actions;
