import type { FieldErrors, UseFormRegister } from "react-hook-form";
import { SectionHeader } from "../../../../components/semanticGraph";
import type { SimulationFormValues } from "../schema";
import { NumberField } from "./NumberField";

interface EntitiesFieldsProps {
  register: UseFormRegister<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
}

export function EntitiesFields({ register, errors }: EntitiesFieldsProps) {
  return (
    <section className='flex flex-col gap-2'>
      <SectionHeader title='Entities' subtitle='panels' />
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='entities.count'
          label='Count'
          register={register}
          errors={errors}
          step={1}
          hint='dof = n - (n_panels + k), so this is not free'
          rules={{
            required: "required",
            validate: (v: number) =>
              (Number.isInteger(v) && v > 0) || "must be a whole number greater than 0"
          }}
        />
        <NumberField
          name='entities.scale_sigma'
          label='Scale sigma'
          register={register}
          errors={errors}
          hint='log-space spread of entity size'
          rules={{
            required: "required",
            validate: (v: number) => v >= 0 || "must be 0 or more"
          }}
        />
      </div>
    </section>
  );
}
