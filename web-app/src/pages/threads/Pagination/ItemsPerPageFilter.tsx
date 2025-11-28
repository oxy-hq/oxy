import React from "react";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/shadcn/select";

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
    <Select
      value={currentLimit.toString()}
      onValueChange={(value) => onLimitChange(Number(value))}
      disabled={isLoading}
    >
      <SelectTrigger
        className="min-w-[70px] justify-between border-border"
        data-testid="threads-per-page-selector"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="end" className="min-w-[70px]">
        {ITEMS_PER_PAGE_OPTIONS.map((option) => (
          <SelectItem key={option} value={option.toString()}>
            {option} / page
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default ItemsPerPageFilter;
