import * as React from "react";
import { Check, ChevronsUpDown } from "lucide-react";
import * as PopoverPrimitive from "@radix-ui/react-popover";

import { cn } from "@/libs/shadcn/utils";
import { Button } from "./button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./command";

const Popover = PopoverPrimitive.Root;
const PopoverTrigger = PopoverPrimitive.Trigger;
const PopoverContent = React.forwardRef<
  React.ElementRef<typeof PopoverPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof PopoverPrimitive.Content>
>(({ className, align = "start", sideOffset = 4, ...props }, ref) => (
  <PopoverPrimitive.Content
    ref={ref}
    align={align}
    sideOffset={sideOffset}
    className={cn(
      "z-50 w-full min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
      className,
    )}
    {...props}
  />
));
PopoverContent.displayName = PopoverPrimitive.Content.displayName;

export type ComboboxStyles = "loading" | "error" | "success";

interface ComboboxProps {
  items: Array<{
    value: string;
    label: string;
    searchText?: string;
    style?: ComboboxStyles;
  }>;
  value?: string;
  onValueChange?: (value: string) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  disabled?: boolean;
  renderItem?: (item: {
    value: string;
    label: string;
    searchText?: string;
  }) => React.ReactNode;
  className?: string;
}

export function Combobox({
  items,
  value,
  onValueChange,
  placeholder = "Select item...",
  searchPlaceholder = "Search...",
  disabled = false,
  renderItem,
  className,
}: ComboboxProps) {
  const [open, setOpen] = React.useState(false);
  const stylesMap: Record<string, string> = {
    loading: "text-blue-700",
    error: "text-red-400",
    success: "text-emerald-600",
  };
  const borderStylesMap: Record<string, string> = {
    loading: "border-blue-700/40",
    error: "border-red-400",
    success: "border-emerald-600",
  };

  const selectedItem = items.find((item) => item.value === value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className={cn(
            "w-full justify-between",
            className,
            borderStylesMap[selectedItem?.style || ""],
          )}
          disabled={disabled}
        >
          {selectedItem ? selectedItem.label : placeholder}
          <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[--radix-popover-trigger-width] p-0">
        <Command>
          <CommandInput placeholder={searchPlaceholder} />
          <CommandList>
            <CommandEmpty>No items found.</CommandEmpty>
            <CommandGroup>
              {items.map((item) => (
                <CommandItem
                  key={item.value}
                  value={item.searchText || item.label}
                  className={cn(stylesMap[item.style || ""])}
                  onSelect={() => {
                    onValueChange?.(item.value);
                    setOpen(false);
                  }}
                >
                  <Check
                    className={cn(
                      "mr-2 h-4 w-4",
                      value === item.value ? "opacity-100" : "opacity-0",
                    )}
                  />
                  {renderItem ? renderItem(item) : item.label}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
