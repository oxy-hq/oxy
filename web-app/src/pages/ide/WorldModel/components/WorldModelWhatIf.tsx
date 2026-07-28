import { useEffect, useMemo, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { usePredict } from "@/hooks/api/useMetricTree";
import type { SensitivityDriver } from "@/types/metricTree";
import { formatDelta, shortMeasureName } from "./measureTarget";
import { InfoTip, MetaBadge, Row, SectionHeader, SectionSpinner } from "./panelPrimitives";

const FORM_HELP: Record<string, string> = {
  linear: "Linear: a fixed change in the driver maps to a fixed change in the target.",
  "log-log":
    "Log-log: a percentage change in the driver maps to a percentage change in the target.",
  "log-linear":
    "Log-linear: a percentage change in the driver maps to a fixed change in the target.",
  "linear-log":
    "Linear-log: a fixed change in the driver maps to a percentage change in the target."
};

const CONFIDENCE_HELP = "The model's confidence in this edge's coefficient.";

/**
 * Size a lever rather than a gap: move one driver by a delta (in that driver's
 * own units) and propagate it up the tree to the target.
 *
 * Propagation is a pure metric-tree walk server-side (no warehouse query), so it
 * is free to re-run — the what-if updates live as you change the driver or the
 * delta rather than behind a manual "run", debounced only to coalesce keystrokes.
 */
export function WorldModelWhatIf({
  drivers,
  target
}: {
  drivers: SensitivityDriver[];
  target: string;
}) {
  // Default to the highest-leverage driver that is actually quantified — a
  // driver with no coefficient cannot propagate anything.
  const quantified = useMemo(
    () => drivers.filter((d) => d.effective_coefficient != null),
    [drivers]
  );
  const [measure, setMeasure] = useState<string>(() => quantified[0]?.measure ?? "");
  const [deltaText, setDeltaText] = useState("1");

  const predict = usePredict();
  const { mutate: runPredict, reset: resetPredict } = predict;
  const selected = quantified.find((d) => d.measure === measure) ?? quantified[0];
  const selectedMeasure = selected?.measure;
  const delta = Number(deltaText);
  const runnable = !!selectedMeasure && Number.isFinite(delta) && delta !== 0;

  // Live propagation: fire whenever the driver or delta settles. An empty or
  // zero delta clears the stale result rather than leaving the last run showing.
  useEffect(() => {
    if (!selectedMeasure) return;
    if (!Number.isFinite(delta) || delta === 0) {
      resetPredict();
      return;
    }
    const handle = window.setTimeout(() => runPredict([{ measure: selectedMeasure, delta }]), 300);
    return () => window.clearTimeout(handle);
  }, [selectedMeasure, delta, runPredict, resetPredict]);

  const impact = predict.data?.impacts.find((i) => i.measure === target);

  if (quantified.length === 0) {
    return (
      <p className='font-mono text-[9.5px] text-muted-foreground leading-relaxed'>
        No driver carries a coefficient, so a what-if cannot be propagated.
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-1.5 pt-1'>
      <div className='flex items-center gap-1'>
        <span className='min-w-0 flex-1'>
          <SectionHeader title='What-if' subtitle='updates live' />
        </span>
        <InfoTip
          content={`Moves the selected driver by Δ, in the driver's own units, and propagates the modelled effect up to ${shortMeasureName(
            target
          )}. A structural estimate from the metric tree, not a forecast.`}
        />
      </div>
      <div className='flex items-center gap-1.5'>
        <span className='shrink-0 font-mono text-muted-foreground text-xs'>Δ</span>
        <Select value={selectedMeasure ?? ""} onValueChange={setMeasure}>
          <SelectTrigger size='sm' className='h-7 flex-1 font-mono text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {quantified.map((d) => (
              <SelectItem key={d.measure} value={d.measure} className='font-mono text-xs'>
                {shortMeasureName(d.measure)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className='shrink-0 font-mono text-muted-foreground text-xs'>by</span>
        <input
          type='number'
          value={deltaText}
          onChange={(e) => setDeltaText(e.target.value)}
          aria-label='Driver delta'
          data-testid='wm-whatif-delta'
          className='h-7 w-16 shrink-0 border border-input bg-transparent px-1.5 font-mono text-xs'
        />
      </div>

      {selected?.form && (
        <p className='font-mono text-[9.5px] text-muted-foreground'>
          {shortMeasureName(selected.measure)} relates to {shortMeasureName(target)} as{" "}
          <span className='text-foreground'>{selected.form}</span>
          {FORM_HELP[selected.form] ? <InfoTip content={FORM_HELP[selected.form]} /> : null}
        </p>
      )}

      {runnable && predict.isPending && <SectionSpinner label='propagating…' />}

      {predict.error && (
        <p className='font-mono text-[10px] text-destructive leading-relaxed'>
          {predict.error instanceof Error ? predict.error.message : "Failed to run the what-if."}
        </p>
      )}

      {runnable &&
        predict.data &&
        (impact ? (
          <Row className='justify-between'>
            <span className='truncate text-muted-foreground'>{shortMeasureName(target)}</span>
            <span className='flex shrink-0 items-center gap-1.5'>
              <span className='text-info'>{formatDelta(impact.estimated_delta)}</span>
              <MetaBadge tooltip={FORM_HELP[impact.form]}>{impact.form}</MetaBadge>
              <MetaBadge tooltip={CONFIDENCE_HELP}>{impact.confidence}</MetaBadge>
            </span>
          </Row>
        ) : (
          <p className='font-mono text-[9.5px] text-muted-foreground leading-relaxed'>
            That move does not propagate to {shortMeasureName(target)}.
          </p>
        ))}
    </div>
  );
}
