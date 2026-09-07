import type { FieldErrors, UseFormRegister } from "react-hook-form";
import { SectionHeader } from "../../../../components/semanticGraph";
import type { SimulationFormValues } from "../schema";
import { NumberField } from "./NumberField";

interface BaselineFieldsProps {
  register: UseFormRegister<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
  /** The names the mechanism section chose. `sales_per_entity_day` is the
   *  *target's* opening level and the driver is what moves it off that level —
   *  neither is "sales" unless this world happens to call them that, so both
   *  labels are written from the live values rather than hardcoded. */
  driver: string;
  target: string;
}

export function BaselineFields({ register, errors, driver, target }: BaselineFieldsProps) {
  return (
    <section className='flex flex-col gap-2'>
      <SectionHeader title='Baseline' />
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='baseline.sales_per_entity_day'
          label={`${target || "Target"} / entity / day`}
          register={register}
          errors={errors}
          hint={`the target's opening level, before any ${driver || "driver"} effect`}
          rules={{
            required: "required",
            validate: (v: number) => v > 0 || "must be greater than 0"
          }}
        />
        <NumberField
          name='baseline.margin'
          label='Margin'
          register={register}
          errors={errors}
          hint='contribution margin, e.g. 0.36'
          rules={{
            required: "required",
            validate: (v: number) => (v > 0 && v < 1) || "must be between 0 and 1"
          }}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='baseline.demand_shock_rho'
          label='Demand shock ρ'
          register={register}
          errors={errors}
          hint='AR(1) persistence — the confounder a legacy policy correlates spend with'
          rules={{
            required: "required",
            validate: (v: number) => Math.abs(v) < 1 || "must be between -1 and 1"
          }}
        />
        <NumberField
          name='baseline.demand_shock_sd'
          label='Demand shock σ'
          register={register}
          errors={errors}
          rules={{
            required: "required",
            validate: (v: number) => v >= 0 || "must be 0 or more"
          }}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='baseline.weekly_seasonality'
          label='Weekly seasonality'
          register={register}
          errors={errors}
          hint='amplitude of the weekly cycle, as a fraction of baseline'
          rules={{
            required: "required",
            validate: (v: number) => v >= 0 || "must be 0 or more"
          }}
        />
        <NumberField
          name='baseline.budget_jitter_sd'
          label='Budget jitter σ'
          register={register}
          errors={errors}
          hint='the identification axis — 0 is the legal "flat lever" corner'
          rules={{
            required: "required",
            validate: (v: number) => v >= 0 || "must be 0 or more (0 is legal)"
          }}
        />
      </div>
    </section>
  );
}
