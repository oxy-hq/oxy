import { useState } from "react";
import { useForm } from "react-hook-form";
import YAML from "yaml";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { useSaveSimulationWorld, useValidateSimulationSpec } from "@/hooks/api/useSimulation";
import { encodeBase64 } from "@/libs/encoding";
import { MIN_FIT_OBSERVATIONS, type SimulationSummary } from "@/types/simulation";
import { BaselineFields } from "./components/BaselineFields";
import { BasicsFields } from "./components/BasicsFields";
import { EntitiesFields } from "./components/EntitiesFields";
import { LeverFields } from "./components/LeverFields";
import { MechanismFields } from "./components/MechanismFields";
import {
  defaultNewWorldValues,
  type SimulationFormValues,
  toSpecInput,
  valuesFromSummary
} from "./schema";

/**
 * Create-or-edit a declared world through a form instead of hand-writing
 * `.simulation.yml`. Every field maps 1:1 onto `SimulationSpec` — see
 * `schema.ts` — and "Save" is gated on the exact same checks the backend
 * runs at run-queue time (`POST /simulations/validate`), so an unreachable
 * optimum or an absorbing lever floor is caught here rather than after a run
 * fails minutes later.
 */
interface SimulationFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Present in edit mode; absent means "create a new world". */
  world?: SimulationSummary;
  /** Every other declared world's name, so create mode can refuse a
   *  duplicate before the round trip to the backend. */
  existingNames: string[];
  onSaved: (name: string) => void;
}

export function SimulationFormDialog({
  open,
  onOpenChange,
  world,
  existingNames,
  onSaved
}: SimulationFormDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='flex max-h-[85vh] flex-col sm:max-w-2xl'>
        {/* Keyed by world so switching from "new" to "edit x" (or between two
            edits) remounts with fresh defaults instead of carrying over the
            previous form's dirty state. */}
        {open && (
          <SimulationFormBody
            key={world?.name ?? "__new__"}
            world={world}
            existingNames={existingNames}
            onCancel={() => onOpenChange(false)}
            onSaved={(name) => {
              onOpenChange(false);
              onSaved(name);
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

interface SimulationFormBodyProps {
  world: SimulationSummary | undefined;
  existingNames: string[];
  onCancel: () => void;
  onSaved: (name: string) => void;
}

function SimulationFormBody({ world, existingNames, onCancel, onSaved }: SimulationFormBodyProps) {
  const [banner, setBanner] = useState<string | null>(null);
  const validateSpec = useValidateSimulationSpec();
  const saveWorld = useSaveSimulationWorld();

  const {
    register,
    control,
    handleSubmit,
    getValues,
    watch,
    setError,
    formState: { errors }
  } = useForm<SimulationFormValues>({
    defaultValues: world ? valuesFromSummary(world) : defaultNewWorldValues()
  });

  const [historyDays, entityCount, lagDays, declaredLagDays, margin, localSlope, driver, target] =
    watch([
      "history_days",
      "entities.count",
      "mechanism.lag_days",
      "mechanism.declared_lag_days",
      "baseline.margin",
      "mechanism.calibrate.local_slope_at_anchor",
      "mechanism.driver",
      "mechanism.target"
    ]);

  const pairsHint = pairedObservationsHint(historyDays, entityCount, lagDays, declaredLagDays);
  const openingReturnHint = openingReturnMessage(margin, localSlope);

  const busy = validateSpec.isPending || saveWorld.isPending;

  const onSubmit = handleSubmit(async (values) => {
    setBanner(null);
    if (!world && existingNames.includes(values.name)) {
      setError("name", { message: "a world with this name already exists" });
      return;
    }

    const spec = toSpecInput(values);
    let result: Awaited<ReturnType<typeof validateSpec.mutateAsync>>;
    try {
      result = await validateSpec.mutateAsync(spec);
    } catch (error) {
      setBanner(error instanceof Error ? error.message : "could not validate this world");
      return;
    }
    if (!result.ok) {
      setBanner(result.error ?? "this world is not valid");
      return;
    }

    const path = world?.file_path ?? `simulations/${spec.name}.simulation.yml`;
    const yaml = YAML.stringify(spec, { indent: 2, lineWidth: 0 });
    try {
      await saveWorld.mutateAsync({
        pathb64: encodeBase64(path),
        yaml,
        isNew: !world
      });
      onSaved(spec.name);
    } catch (error) {
      setBanner(error instanceof Error ? error.message : "failed to save the file");
    }
  });

  return (
    <>
      <DialogHeader>
        <DialogTitle className='font-mono text-sm'>
          {world ? `Edit ${world.name}` : "New world"}
        </DialogTitle>
      </DialogHeader>

      <form
        id='simulation-form'
        onSubmit={onSubmit}
        className='flex flex-col gap-4 overflow-y-auto px-1'
      >
        {banner && (
          <p className='rounded-md border border-destructive/40 bg-destructive/10 p-2 text-[11px] text-destructive leading-relaxed'>
            {banner}
          </p>
        )}
        <BasicsFields register={register} errors={errors} nameLocked={Boolean(world)} />
        <EntitiesFields register={register} errors={errors} />
        <BaselineFields register={register} errors={errors} driver={driver} target={target} />
        <MechanismFields
          register={register}
          control={control}
          errors={errors}
          getValues={getValues}
          pairsHint={pairsHint}
          openingReturnHint={openingReturnHint}
          driver={driver}
          target={target}
        />
        <LeverFields register={register} errors={errors} getValues={getValues} />
      </form>

      <DialogFooter>
        <Button type='button' variant='ghost' size='sm' onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
        <Button type='submit' form='simulation-form' size='sm' disabled={busy}>
          {busy ? "Checking…" : world ? "Save" : "Create"}
        </Button>
      </DialogFooter>
    </>
  );
}

/** "N paired observations after the lag (floor is 30)" — the fitter pairs day
 *  `d`'s driver with day `d + lag`'s target within a panel, so this is what
 *  `history_days` and `entities.count` actually buy against the declared lag. */
function pairedObservationsHint(
  historyDays: number,
  entityCount: number,
  lagDays: number,
  declaredLagDays: string
): string {
  const declared = declaredLagDays.trim() === "" ? lagDays : Number(declaredLagDays);
  if (
    !Number.isFinite(historyDays) ||
    !Number.isFinite(entityCount) ||
    !Number.isFinite(declared)
  ) {
    return "";
  }
  const pairs = Math.max(0, historyDays - declared) * entityCount;
  return `${pairs} paired observations after the declared lag (floor is ${MIN_FIT_OBSERVATIONS})`;
}

/** "opening return: margin × slope = X — needs > 1" — the curve's own
 *  precondition, from `crates/simulation/src/spec/curve.rs`. */
function openingReturnMessage(margin: number, localSlope: number): string {
  if (!Number.isFinite(margin) || !Number.isFinite(localSlope)) return "";
  const opening = margin * localSlope;
  return `opening return: margin × slope = ${opening.toFixed(2)} — needs to be > 1`;
}
