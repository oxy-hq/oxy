import type { FieldErrors, UseFormRegister } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { SectionHeader } from "../../../../components/semanticGraph";
import type { SimulationFormValues } from "../schema";
import { NumberField } from "./NumberField";
import { TextField } from "./TextField";

interface BasicsFieldsProps {
  register: UseFormRegister<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
  /** Locked once a world exists — renaming would orphan the run history that
   *  references it by name. */
  nameLocked: boolean;
}

export function BasicsFields({ register, errors, nameLocked }: BasicsFieldsProps) {
  return (
    <section className='flex flex-col gap-2'>
      <SectionHeader title='Basics' />
      <TextField
        name='name'
        label='Name'
        register={register}
        errors={errors}
        placeholder='moderate_confounding'
        disabled={nameLocked}
        hint={
          nameLocked
            ? undefined
            : "letters, numbers, underscores or hyphens — becomes simulations/<name>.simulation.yml"
        }
        rules={{
          required: "a world needs a name",
          pattern: {
            value: /^[A-Za-z0-9_-]+$/,
            message: "letters, numbers, underscores or hyphens only"
          }
        }}
      />
      <div className='space-y-1'>
        <Label htmlFor='description' className='text-xs'>
          Description
        </Label>
        <Textarea
          id='description'
          className='min-h-16 text-xs'
          placeholder="what this world is for, and what it's measured at"
          {...register("description")}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='seed'
          label='Seed'
          register={register}
          errors={errors}
          step={1}
          rules={{ required: "required", validate: isNonNegativeInteger }}
        />
        <NumberField
          name='replicates'
          label='Replicates'
          register={register}
          errors={errors}
          step={1}
          hint='draws before a cell of the outcome map means anything'
          rules={{ required: "required", validate: isPositiveInteger }}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='periods'
          label='Periods'
          register={register}
          errors={errors}
          step={1}
          hint='decision periods the loop runs for'
          rules={{ required: "required", validate: isPositiveInteger }}
        />
        <NumberField
          name='period_days'
          label='Period days'
          register={register}
          errors={errors}
          step={1}
          rules={{ required: "required", validate: isPositiveInteger }}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='history_days'
          label='History days'
          register={register}
          errors={errors}
          step={1}
          hint='days generated under the opening spend before the loop starts'
          rules={{ required: "required", validate: isPositiveInteger }}
        />
        <TextField
          name='start_date'
          label='Start date'
          type='date'
          register={register}
          errors={errors}
          rules={{ required: "required" }}
        />
      </div>
    </section>
  );
}

function isPositiveInteger(value: number) {
  return (Number.isInteger(value) && value > 0) || "must be a whole number greater than 0";
}

function isNonNegativeInteger(value: number) {
  return (Number.isInteger(value) && value >= 0) || "must be a whole number, 0 or more";
}
