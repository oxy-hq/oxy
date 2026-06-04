import { Check } from "lucide-react";
import type React from "react";
import { cn } from "@/libs/shadcn/utils";

interface ConnectorCardProps {
  label: string;
  description: string;
  selected: boolean;
  onSelect: () => void;
  /** Optional stable selector hook for tests. */
  testId?: string;
}

/** Selectable card used in the source / destination steps of the
 *  new-pipeline wizard. */
const ConnectorCard: React.FC<ConnectorCardProps> = ({
  label,
  description,
  selected,
  onSelect,
  testId
}) => (
  <button
    type='button'
    onClick={onSelect}
    data-testid={testId}
    aria-pressed={selected}
    className={cn(
      "relative flex flex-col gap-1 rounded-lg border p-3 text-left transition-colors",
      selected
        ? "border-primary bg-primary/5 ring-1 ring-primary"
        : "border-border hover:border-primary/50 hover:bg-muted/50"
    )}
  >
    {selected && (
      <Check className='absolute top-2 right-2 h-4 w-4 text-primary' aria-hidden='true' />
    )}
    <span className='font-medium text-sm'>{label}</span>
    <span className='text-muted-foreground text-xs'>{description}</span>
  </button>
);

export default ConnectorCard;
