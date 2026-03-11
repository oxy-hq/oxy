import { useId } from "react";
import type { ControlConfig } from "@/types/app";

type Props = {
  control: ControlConfig;
  value: string;
  onChange: (value: string) => void;
};

export function DateControl({ control, value, onChange }: Props) {
  const inputId = useId();
  return (
    <div className='flex flex-col gap-1'>
      {control.label && (
        <label htmlFor={inputId} className='font-medium text-muted-foreground text-xs'>
          {control.label}
        </label>
      )}
      <input
        id={inputId}
        type='date'
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value)}
        className='h-9 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
      />
    </div>
  );
}
