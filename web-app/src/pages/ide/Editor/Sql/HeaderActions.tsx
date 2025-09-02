import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Download, Loader2, Play } from "lucide-react";
import DatabasesDropdown from "./DatabasesDropdown";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { handleDownloadFile } from "@/libs/utils/string";

interface HeaderActionsProps {
  onExecuteSql: (database: string) => void;
  loading: boolean;
  sql: string;
}

const HeaderActions = ({ onExecuteSql, sql, loading }: HeaderActionsProps) => {
  const [database, setDatabase] = useState<string | null>(null);

  const handleExecuteSql = () => {
    onExecuteSql(database ?? "");
  };

  const handleDownloadSql = () => {
    const blob = new Blob([sql], { type: "text/plain" });
    handleDownloadFile(blob, "query.sql");
  };

  return (
    // actions should stay in one row; when constrained they scroll horizontally
    <div className="flex items-center gap-2 whitespace-nowrap overflow-x-auto">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            title="Download SQL"
            variant="outline"
            size="icon"
            onClick={handleDownloadSql}
          >
            <Download className="h-4 w-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Download the SQL query</TooltipContent>
      </Tooltip>

      <DatabasesDropdown
        onSelect={(database) => setDatabase(database)}
        database={database}
      />
      <Button
        className="hover:text-muted-foreground flex-shrink-0"
        variant="ghost"
        disabled={loading || !database}
        onClick={handleExecuteSql}
        title="Run query"
      >
        {loading ? (
          <Loader2 className="w-4 h-4 animate-[spin_0.3s_linear_infinite]" />
        ) : (
          <Play className="w-4 h-4" />
        )}
      </Button>
    </div>
  );
};

export default HeaderActions;
