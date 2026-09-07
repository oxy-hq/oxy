import get from "lodash/get";
import type {
  FieldErrors,
  FieldPath,
  FieldValues,
  RegisterOptions,
  UseFormRegister
} from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";

interface TextFieldProps<T extends FieldValues, TName extends FieldPath<T>> {
  name: TName;
  label: string;
  register: UseFormRegister<T>;
  errors: FieldErrors<T>;
  rules?: RegisterOptions<T, TName>;
  placeholder?: string;
  /** `"number"` renders a numeric input (spinner, numeric keyboard) while
   *  still registering the raw string — for an optional numeric field that
   *  needs to distinguish "blank" from any parsed value, which RHF's
   *  `valueAsNumber` can't (an empty field becomes `NaN`, not `""`). */
  type?: "text" | "date" | "number";
  hint?: string;
  disabled?: boolean;
}

// See `NumberField` for why this is generic over the specific path (`TName`)
// and not just the form's value type.
export function TextField<T extends FieldValues, TName extends FieldPath<T> = FieldPath<T>>({
  name,
  label,
  register,
  errors,
  rules,
  placeholder,
  type = "text",
  hint,
  disabled
}: TextFieldProps<T, TName>) {
  const error = get(errors, name)?.message as string | undefined;

  return (
    <div className='space-y-1'>
      <Label htmlFor={name} className='text-xs'>
        {label}
      </Label>
      <Input
        id={name}
        type={type}
        placeholder={placeholder}
        className='h-8 font-mono text-xs'
        aria-invalid={Boolean(error)}
        // Native `disabled`, not RHF's `register(..., { disabled })` — the
        // latter drops the field's value from the submitted data, and a
        // locked name still has to travel with the rest of the form.
        disabled={disabled}
        {...register(name, rules)}
      />
      {error ? (
        <p className='text-[10px] text-destructive'>{error}</p>
      ) : (
        hint && <p className='text-[10px] text-muted-foreground'>{hint}</p>
      )}
    </div>
  );
}
