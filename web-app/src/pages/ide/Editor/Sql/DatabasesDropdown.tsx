import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { DropdownMenu } from "@/components/ui/shadcn/dropdown-menu";
import { ChevronDown, Loader2 } from "lucide-react";
import { useEffect, useMemo } from "react";
import useDatabases from "@/hooks/api/useDatabases";

interface DatabaseDropdownProps {
  onSelect: (database: string) => void;
  database: string | null;
}

const DatabasesDropdown = ({ onSelect, database }: DatabaseDropdownProps) => {
  const { data: databases, isLoading, isSuccess } = useDatabases();

  const databaseOptions = useMemo(
    () =>
      databases
        ?.map((database) => ({
          id: database,
          name: database,
        }))
        .sort((a, b) => a.name.localeCompare(b.name)) ?? [],
    [databases],
  );

  useEffect(() => {
    if (isSuccess && databases && databases.length > 0 && !database) {
      onSelect(databaseOptions[0].id);
    }
  }, [isSuccess, databases, databaseOptions, onSelect, database]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger>
        <Button size="sm" disabled={isLoading} variant="outline">
          <span>{database}</span>
          {isLoading ? <Loader2 className="animate-spin" /> : <ChevronDown />}
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

export default DatabasesDropdown;
