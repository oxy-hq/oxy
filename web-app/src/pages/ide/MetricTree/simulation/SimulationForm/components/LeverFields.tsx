import { ChevronRight } from "lucide-react";
import { useState } from "react";
import type { FieldErrors, UseFormGetValues, UseFormRegister } from "react-hook-form";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { cn } from "@/libs/shadcn/utils";
import { SectionHeader } from "../../../../components/semanticGraph";
import type { SimulationFormValues } from "../schema";
import { NumberField } from "./NumberField";

interface LeverFieldsProps {
  register: UseFormRegister<SimulationFormValues>;
  errors: FieldErrors<SimulationFormValues>;
  getValues: UseFormGetValues<SimulationFormValues>;
}

/**
 * Collapsed by default: every value here already has a sane default
 * (`DEFAULT_LEVER`), and these are policy constraints — what a `machine` run
 * is allowed to do — not properties of the world most authors are trying to
 * declare. Opening it is for the person who specifically wants a wider or
 * narrower lever, e.g. `wide_lever.simulation.yml`.
 */
export function LeverFields({ register, errors, getValues }: LeverFieldsProps) {
  const [open, setOpen] = useState(false);

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex w-full items-center gap-1.5 text-left'>
        <ChevronRight className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")} />
        <SectionHeader title='Lever' subtitle='advanced · policy constraints' />
      </CollapsibleTrigger>
      <CollapsibleContent className='flex flex-col gap-2 pt-2 pl-4'>
        <div className='grid grid-cols-2 gap-2'>
          <NumberField
            name='lever.min_multiple'
            label='Min multiple'
            register={register}
            errors={errors}
            hint='floor on spend, as a multiple of the anchor — never 0'
            rules={{
              required: "required",
              validate: (v: number) =>
                (v > 0 && v < getValues("lever.max_multiple")) ||
                "must be greater than 0 and below max multiple"
            }}
          />
          <NumberField
            name='lever.max_multiple'
            label='Max multiple'
            register={register}
            errors={errors}
            hint='ceiling on spend, as a multiple of the anchor'
            rules={{
              required: "required",
              validate: (v: number) =>
                v > getValues("lever.min_multiple") || "must be above min multiple"
            }}
          />
        </div>
        <div className='grid grid-cols-2 gap-2'>
          <NumberField
            name='lever.max_move_per_period'
            label='Max move / period'
            register={register}
            errors={errors}
            hint='largest fractional change one decision period may make'
            rules={{
              required: "required",
              validate: (v: number) => (v > 0 && v < 1) || "must be between 0 and 1"
            }}
          />
          <NumberField
            name='lever.explore_jitter_sd'
            label='Explore jitter σ'
            register={register}
            errors={errors}
            hint='log-space spread of the machine+explore jitter'
            rules={{
              required: "required",
              validate: (v: number) => v >= 0 || "must be 0 or more"
            }}
          />
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
