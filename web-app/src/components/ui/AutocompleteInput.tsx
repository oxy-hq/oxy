import React from "react";
import { Input } from "@/components/ui/shadcn/input";

interface AutocompleteInputProps extends Omit<
  React.ComponentProps<typeof Input>,
  "list"
> {
  options: string[];
  datalistId: string;
}

export const AutocompleteInput = React.forwardRef<
  HTMLInputElement,
  AutocompleteInputProps
>(({ options, datalistId, ...props }, ref) => {
  return (
    <>
      <Input ref={ref} list={datalistId} {...props} />
      <datalist id={datalistId}>
        {options.map((option) => (
          <option key={option} value={option} />
        ))}
      </datalist>
    </>
  );
});

AutocompleteInput.displayName = "AutocompleteInput";
