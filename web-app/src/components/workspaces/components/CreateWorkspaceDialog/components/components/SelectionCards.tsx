import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { cn } from "@/libs/shadcn/utils";
import type { SelectionOption } from "../types";

interface SelectionCardsProps {
  options: SelectionOption[];
  onSelect: (id: string) => void;
  selectedId?: string;
  disabled?: boolean;
  collapseAfter?: number;
}

export default function SelectionCards({
  options,
  onSelect,
  selectedId,
  disabled,
  collapseAfter
}: SelectionCardsProps) {
  const hasCollapse =
    typeof collapseAfter === "number" && collapseAfter > 0 && collapseAfter < options.length;
  const [expanded, setExpanded] = useState(false);
  const visibleOptions = hasCollapse && !expanded ? options.slice(0, collapseAfter) : options;

  return (
    <div className='flex flex-col gap-3'>
      <div className='grid grid-cols-2 gap-3'>
        {visibleOptions.map((option) => {
          const isSelected = selectedId === option.id;
          return (
            <button
              key={option.id}
              type='button'
              onClick={() => !disabled && onSelect(option.id)}
              disabled={disabled}
              className={cn(
                "flex flex-col gap-1 rounded-lg border p-4 text-left transition-all",
                "hover:border-primary/50 hover:bg-primary/5",
                isSelected && "border-primary bg-primary/10",
                !isSelected && "border-border bg-card",
                disabled && "cursor-not-allowed opacity-50"
              )}
            >
              <span className='font-medium text-sm'>{option.label}</span>
              <span className='text-muted-foreground text-xs'>{option.description}</span>
            </button>
          );
        })}
      </div>
      {hasCollapse && !expanded && (
        <button
          type='button'
          onClick={() => setExpanded(true)}
          className='flex items-center gap-1 self-start text-muted-foreground text-xs transition-colors hover:text-foreground'
        >
          More
          <ChevronDown className='h-3 w-3' />
        </button>
      )}
    </div>
  );
}
