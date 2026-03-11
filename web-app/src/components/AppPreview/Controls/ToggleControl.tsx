import { Switch } from "@/components/ui/shadcn/switch";
import type { ControlConfig } from "@/types/app";

type Props = {
  control: ControlConfig;
  value: boolean;
  onChange: (value: boolean) => void;
};

export function ToggleControl({ control, value, onChange }: Props) {
  return (
    <div className='flex flex-col gap-1'>
      {control.label && (
        <label htmlFor={control.name} className='font-medium text-muted-foreground text-xs'>
          {control.label}
        </label>
      )}
      <div className='flex h-8 items-center'>
        <Switch id={control.name} checked={value} onCheckedChange={onChange} />
      </div>
    </div>
  );
}
