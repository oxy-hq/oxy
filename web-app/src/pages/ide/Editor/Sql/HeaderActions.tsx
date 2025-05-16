import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Loader2, Play } from "lucide-react";
import DatabasesDropdown from "./DatabasesDropdown";

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
    <div className="flex gap-2 md:flex-row flex-col items-start">
      <DatabasesDropdown
        onSelect={(database) => setDatabase(database)}
        database={database}
      />
      <Button
        className="text-white hover:text-muted-foreground"
        variant="ghost"
        disabled={loading || !database}
        onClick={handleExecuteSql}
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
