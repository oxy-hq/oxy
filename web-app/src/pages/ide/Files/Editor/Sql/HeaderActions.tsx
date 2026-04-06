import { Play } from "lucide-react";
import { useState } from "react";
import DatabaseSelector from "@/components/sql/DatabaseSelector";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";

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
    <div className='flex items-center gap-2 overflow-x-auto whitespace-nowrap'>
      <DatabaseSelector onSelect={setDatabase} database={database} />
      <Button
        size='sm'
        className='flex-shrink-0 hover:text-muted-foreground'
        variant='ghost'
        disabled={loading || !database}
        onClick={handleExecuteSql}
        title='Run query'
      >
        {loading ? <Spinner /> : <Play className='h-4 w-4' />}
      </Button>
    </div>
  );
};

export default HeaderActions;
