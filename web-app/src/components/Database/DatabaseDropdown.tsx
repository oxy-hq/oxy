import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { DropdownMenu } from "@/components/ui/shadcn/dropdown-menu";
import { DatabaseInfo } from "@/types/database";
import { ChevronDown, Loader2, Database } from "lucide-react";
import { useEffect } from "react";

type Props = {
  onSelect: (database: DatabaseInfo) => void;
  database: DatabaseInfo | null;
  databases: DatabaseInfo[];
  isLoading: boolean;
  disabled?: boolean;
};

const DatabaseDropdown = ({
  onSelect,
  database,
  databases,
  isLoading,
  disabled = false,
}: Props) => {
  useEffect(() => {
    if (!isLoading && databases && databases.length > 0 && !database) {
      onSelect(databases[0]);
    }
  }, [isLoading, databases, onSelect, database]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger disabled={isLoading || disabled}>
        <Button
          disabled={isLoading || disabled}
          variant="outline"
          className="bg-sidebar-background border-sidebar-background"
        >
          <Database className="h-4 w-4" />
          <span>{database?.name || "Select Database"}</span>
          {isLoading ? (
            <Loader2 className="animate-spin h-4 w-4" />
          ) : (
            <ChevronDown className="h-4 w-4" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="customScrollbar">
        {databases.map((item) => (
          <DropdownMenuCheckboxItem
            key={item.name}
            checked={item.name === database?.name}
            onCheckedChange={() => onSelect(item)}
          >
            <div className="flex flex-col">
              <span className="font-medium">{item.name}</span>
              <span className="text-xs text-muted-foreground">
                {item.dialect}
              </span>
            </div>
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default DatabaseDropdown;
