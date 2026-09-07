import get from "lodash/get";
import { useState } from "react";
import type { Control, FieldErrors, RegisterOptions } from "react-hook-form";
import { useController } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { SimulationFormValues } from "../schema";

/** Sentinel for the "Custom…" item. Radix forbids an empty `SelectItem`
 *  value, and no legal column name looks like this. */
const CUSTOM = "__custom__";

interface ColumnNameFieldProps {
  name: "mechanism.driver" | "mechanism.target";
  label: string;
  control: Control<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
  /** The curated names for this side of the mechanism — see
   *  `DRIVER_COLUMN_NAMES` / `TARGET_COLUMN_NAMES` for why they are a list. */
  options: readonly string[];
  rules?: RegisterOptions<SimulationFormValues, "mechanism.driver" | "mechanism.target">;
  hint?: string;
}

/**
 * Picks the column name for one side of the mechanism.
 *
 * A list rather than a text box because the two ends are not interchangeable:
 * the generated world always runs one fixed mechanism (a lagged spend with
 * diminishing returns lifting a revenue level, scored as
 * `target − prime_cost − driver`), and these fields only name its columns.
 * "Custom…" stays reachable because the backend accepts any bare column name
 * and an existing `.simulation.yml` may already carry one — a world loaded
 * with a name outside the list opens in that mode rather than being silently
 * rewritten to a preset.
 */
export function ColumnNameField({
  name,
  label,
  control,
  errors,
  options,
  rules,
  hint
}: ColumnNameFieldProps) {
  const { field } = useController({ name, control, rules });
  const [custom, setCustom] = useState(
    () => Boolean(field.value) && !options.includes(field.value)
  );
  const error = get(errors, name)?.message as string | undefined;

  return (
    <div className='space-y-1'>
      <Label htmlFor={name} className='text-xs'>
        {label}
      </Label>
      <Select
        value={custom ? CUSTOM : field.value}
        onValueChange={(next) => {
          if (next === CUSTOM) {
            setCustom(true);
            // Cleared rather than carried over, so the revealed input reads as
            // "type a name" instead of pre-filling a preset the author just
            // chose to leave.
            field.onChange("");
            return;
          }
          setCustom(false);
          field.onChange(next);
        }}
      >
        <SelectTrigger
          id={custom ? undefined : name}
          size='sm'
          className='h-8 w-full font-mono text-xs'
          aria-invalid={Boolean(error)}
        >
          <SelectValue placeholder='choose a column' />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option} value={option} className='font-mono text-xs'>
              {option}
            </SelectItem>
          ))}
          <SelectItem value={CUSTOM} className='text-xs'>
            Custom…
          </SelectItem>
        </SelectContent>
      </Select>
      {custom && (
        <Input
          id={name}
          placeholder={options[0]}
          className='h-8 font-mono text-xs'
          aria-invalid={Boolean(error)}
          {...field}
        />
      )}
      {error ? (
        <p className='text-[10px] text-destructive'>{error}</p>
      ) : (
        hint && <p className='text-[10px] text-muted-foreground'>{hint}</p>
      )}
    </div>
  );
}
