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

/**
 * One labelled numeric input, shared by every field-group in the simulation
 * form. Every declared world is ~20 of these — the repetition belongs in one
 * place, not copy-pasted per group.
 */
interface NumberFieldProps<T extends FieldValues, TName extends FieldPath<T>> {
  name: TName;
  label: string;
  register: UseFormRegister<T>;
  errors: FieldErrors<T>;
  rules?: RegisterOptions<T, TName>;
  step?: number | "any";
  /** Shown under the field while it has no error — a live-computed reading
   *  (paired observations, opening return) that helps before the round trip
   *  to `POST /simulations/validate` ever happens. */
  hint?: string;
}

// Generic over the specific field path (`TName`), not just the form's value
// type (`T`) — narrowing to `FieldPath<T>` alone would type every `validate`
// callback's argument as the union of every field's value in the whole form,
// since RHF's `Validate<...>` is keyed off the exact path.
export function NumberField<T extends FieldValues, TName extends FieldPath<T> = FieldPath<T>>({
  name,
  label,
  register,
  errors,
  rules,
  step = "any",
  hint
}: NumberFieldProps<T, TName>) {
  const error = get(errors, name)?.message as string | undefined;

  return (
    <div className='space-y-1'>
      <Label htmlFor={name} className='text-xs'>
        {label}
      </Label>
      <Input
        id={name}
        type='number'
        step={step}
        className='h-8 text-xs'
        aria-invalid={Boolean(error)}
        // `valueAsNumber` last: it must always win over anything a caller's
        // `rules` sets — this field is always numeric. The cast is because
        // RHF's `RegisterOptions` is conditionally typed on the *specific*
        // field's value (e.g. `pattern` is disallowed once `valueAsNumber` is
        // set), which a component generic over `TName` can't express; every
        // caller here only ever passes `required`/`validate`/`min`/`max`.
        {...register(name, { ...rules, valueAsNumber: true } as RegisterOptions<T, TName>)}
      />
      {error ? (
        <p className='text-[10px] text-destructive'>{error}</p>
      ) : (
        hint && <p className='text-[10px] text-muted-foreground'>{hint}</p>
      )}
    </div>
  );
}
