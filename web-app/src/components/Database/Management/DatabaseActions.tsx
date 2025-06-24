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

interface DatabaseActionsProps {
  database: DatabaseInfo;
  isLoading: boolean;
  onSync: (datasets?: string[]) => void;
}

export const DatabaseActions: React.FC<DatabaseActionsProps> = ({
  database,
  isLoading,
  onSync,
}) => {
  const datasetKeys = Object.keys(database.datasets);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0"
          disabled={isLoading}
        >
          {isLoading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <MoreHorizontal className="h-4 w-4" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => onSync()} disabled={isLoading}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Sync All Datasets
        </DropdownMenuItem>
        {datasetKeys.map((dataset) => (
          <DropdownMenuItem
            key={dataset}
            onClick={() => onSync([dataset])}
            disabled={isLoading}
          >
            <TableIcon className="h-4 w-4 mr-2" />
            Sync {dataset}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
