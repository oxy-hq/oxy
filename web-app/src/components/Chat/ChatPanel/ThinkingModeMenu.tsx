import { Switch } from "@/components/ui/shadcn/switch";
import type { ThinkingMode } from "@/services/api/analytics";

const ThinkingModeMenu = ({
  value,
  onChange,
  disabled
}: {
  value: ThinkingMode;
  onChange: (mode: ThinkingMode) => void;
  disabled: boolean;
}) => (
  <div className='flex items-center gap-2'>
    <span className='text-muted-foreground text-xs'>Extended Thinking</span>
    <Switch
      checked={value === "extended_thinking"}
      onCheckedChange={(checked) => onChange(checked ? "extended_thinking" : "auto")}
      disabled={disabled}
      data-testid='thinking-mode-trigger'
    />
  </div>
);

export default ThinkingModeMenu;
