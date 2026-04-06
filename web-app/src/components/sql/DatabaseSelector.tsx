import { useEffect, useMemo } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useDatabases from "@/hooks/api/databases/useDatabases";
import type { DatabaseInfo } from "@/types/database";

export interface DatabaseSelectorProps {
  onSelect: (database: string) => void;
  database: string | null;
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
    <Select value={database ?? ""} onValueChange={(id) => onSelect(id)} disabled={isLoading}>
      <SelectTrigger size='sm' className={className}>
        {isLoading ? <Spinner /> : <SelectValue placeholder={placeholder} />}
      </SelectTrigger>
      <SelectContent>
        {databaseOptions.map((item) => (
          <SelectItem className='cursor-pointer' key={item.id} value={item.id}>
            {item.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default DatabaseSelector;
