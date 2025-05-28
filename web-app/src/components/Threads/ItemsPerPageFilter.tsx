import React from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { Button } from "@/components/ui/shadcn/button";
import { ChevronDown } from "lucide-react";

interface ItemsPerPageFilterProps {
  currentLimit: number;
  onLimitChange: (limit: number) => void;
  isLoading?: boolean;
}

const ITEMS_PER_PAGE_OPTIONS = [10, 25, 50, 100];

const ItemsPerPageFilter: React.FC<ItemsPerPageFilterProps> = ({
  currentLimit,
  onLimitChange,
  isLoading = false,
}) => {
  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted-foreground">Items per page:</span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="min-w-[70px] justify-between"
            disabled={isLoading}
          >
            {currentLimit}
            <ChevronDown className="h-4 w-4 opacity-50" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-[70px]">
          {ITEMS_PER_PAGE_OPTIONS.map((option) => (
            <DropdownMenuItem
              key={option}
              onClick={() => onLimitChange(option)}
              className={currentLimit === option ? "bg-accent" : ""}
            >
              {option}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
};

export default ItemsPerPageFilter;
