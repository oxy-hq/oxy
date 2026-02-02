import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Loader2, Play } from "lucide-react";
import DatabaseSelector from "@/components/sql/DatabaseSelector";

interface HeaderActionsProps {
  onExecuteSql: (database: string) => void;
  loading: boolean;
}

const HeaderActions = ({ onExecuteSql, loading }: HeaderActionsProps) => {
  const [database, setDatabase] = useState<string | null>(null);

  const handleExecuteSql = () => {
    onExecuteSql(database ?? "");
  };

  return (
    // actions should stay in one row; when constrained they scroll horizontally
    <div className="flex items-center gap-2 whitespace-nowrap overflow-x-auto">
      <DatabaseSelector onSelect={setDatabase} database={database} />
      <Button
        size="sm"
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
