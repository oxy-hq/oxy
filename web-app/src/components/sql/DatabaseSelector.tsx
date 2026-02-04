import { ChevronDown, Loader2 } from "lucide-react";
import { useEffect, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import useDatabases from "@/hooks/api/databases/useDatabases";
import type { DatabaseInfo } from "@/types/database";

export interface DatabaseSelectorProps {
  onSelect: (database: string) => void;
  database: string | null;
  variant?: "dropdown" | "select";
  placeholder?: string;
  className?: string;
}

const DatabaseSelector = ({
  onSelect,
  database,
  placeholder = "Select database",
  className
}: DatabaseSelectorProps) => {
  const { data: databases, isLoading, isSuccess } = useDatabases();

  const databaseOptions = useMemo(
    () =>
      databases
        ?.map((databaseInfo: DatabaseInfo) => ({
          id: databaseInfo.name,
          name: databaseInfo.name
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [databases]
  );

  useEffect(() => {
    if (isSuccess && databases && databases.length > 0 && !database) {
      onSelect(databaseOptions[0].id);
    }
  }, [isSuccess, databases, databaseOptions, onSelect, database]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size='sm' disabled={isLoading} variant='outline' className={className}>
          <span className='block max-w-[120px] truncate'>{database ?? placeholder}</span>
          {isLoading ? <Loader2 className='animate-spin' /> : <ChevronDown />}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        {databaseOptions.map((item) => (
          <DropdownMenuCheckboxItem
            key={item.id}
            checked={item.id === database}
            onCheckedChange={() => onSelect(item.id)}
          >
            {item.name}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default DatabaseSelector;
