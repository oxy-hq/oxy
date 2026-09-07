import type { Control, FieldErrors, UseFormGetValues, UseFormRegister } from "react-hook-form";
import { SectionHeader } from "../../../../components/semanticGraph";
import {
  columnNameError,
  DRIVER_COLUMN_NAMES,
  type SimulationFormValues,
  TARGET_COLUMN_NAMES
} from "../schema";
import { ColumnNameField } from "./ColumnNameField";
import { NumberField } from "./NumberField";
import { TextField } from "./TextField";

interface MechanismFieldsProps {
  register: UseFormRegister<SimulationFormValues>;
  control: Control<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
  getValues: UseFormGetValues<SimulationFormValues>;
  /** "N paired observations, floor is 30" — recomputed by the parent from
   *  `history_days`, `entities.count` and whichever lag is declared, since
   *  those three fields live in three different sections. */
  pairsHint: string;
  /** "opening return: margin × slope = X" — same cross-section story for the
   *  curve's opening return, computed from `baseline.margin`. */
  openingReturnHint: string;
  /** The names currently chosen for the two columns, so this section's own
   *  header can say what the edge under test actually is. */
  driver: string;
  target: string;
}

export function MechanismFields({
  register,
  control,
  errors,
  getValues,
  pairsHint,
  openingReturnHint,
  driver,
  target
}: MechanismFieldsProps) {
  return (
    <section className='flex flex-col gap-2'>
      <SectionHeader title='Mechanism' subtitle={`${driver || "driver"} → ${target || "target"}`} />
      <div className='grid grid-cols-2 gap-2'>
        <ColumnNameField
          name='mechanism.driver'
          label='Driver'
          control={control}
          errors={errors}
          options={DRIVER_COLUMN_NAMES}
          hint='the spend the lever moves'
          rules={{
            required: "required",
            validate: (v: string) => validateColumnName(v, "driver", getValues)
          }}
        />
        <ColumnNameField
          name='mechanism.target'
          label='Target'
          control={control}
          errors={errors}
          options={TARGET_COLUMN_NAMES}
          hint='the revenue that spend lifts'
          rules={{
            required: "required",
            validate: (v: string) => validateColumnName(v, "target", getValues)
          }}
        />
      </div>
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='mechanism.lag_days'
          label='Lag days'
          register={register}
          errors={errors}
          step={1}
          hint={`the truth — days between ${driver || "the driver"} and the ${target || "target"} it produces`}
          rules={{
            required: "required",
            validate: (v: number) =>
              (Number.isInteger(v) && v > 0) || "must be a whole number greater than 0"
          }}
        />
        <TextField
          name='mechanism.declared_lag_days'
          label='Declared lag days'
          type='number'
          register={register}
          errors={errors}
          placeholder='same as lag days'
          hint={pairsHint}
          rules={{
            validate: (v: string) => {
              if (v.trim() === "") return true;
              const n = Number(v);
              return (
                (Number.isInteger(n) && n > 0) || "leave blank, or a whole number greater than 0"
              );
            }
          }}
        />
      </div>
      <NumberField
        name='mechanism.noise_ratio'
        label='Noise ratio'
        register={register}
        errors={errors}
        hint='noise on the target, as a fraction of the baseline level'
        rules={{
          required: "required",
          validate: (v: number) => v >= 0 || "must be 0 or more"
        }}
      />
      <SectionHeader title='Calibrate' subtitle='solves the response curve' />
      <div className='grid grid-cols-2 gap-2'>
        <NumberField
          name='mechanism.calibrate.anchor_spend_share'
          label='Anchor spend share'
          register={register}
          errors={errors}
          hint={`reference spend, as a share of baseline daily ${target || "target"}`}
          rules={{
            required: "required",
            validate: (v: number) => v > 0 || "must be greater than 0"
          }}
        />
        <NumberField
          name='mechanism.calibrate.local_slope_at_anchor'
          label='Local slope at anchor'
          register={register}
          errors={errors}
          hint={openingReturnHint}
          rules={{
            required: "required",
            validate: (v: number) => v > 0 || "must be greater than 0"
          }}
        />
      </div>
      <NumberField
        name='mechanism.calibrate.optimum_at'
        label='Optimum at'
        register={register}
        errors={errors}
        hint='where the profit optimum should sit, as a multiple of the anchor spend'
        rules={{
          required: "required",
          validate: (v: number) => v > 0 || "must be greater than 0"
        }}
      />
    </section>
  );
}

function validateColumnName(
  value: string,
  field: "driver" | "target",
  getValues: UseFormGetValues<SimulationFormValues>
): true | string {
  const other = field === "driver" ? getValues("mechanism.target") : getValues("mechanism.driver");
  return columnNameError(value, other) ?? true;
}
