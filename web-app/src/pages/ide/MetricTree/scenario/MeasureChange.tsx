import { cn } from "@/libs/shadcn/utils";
import type { ImpactConfidence } from "@/types/metricTree";
import { ConfidenceMark } from "./ConfidenceMark";
import { formatPercent } from "./nodeValue";

interface MeasureChangeProps {
  baseline?: number;
  simulated?: number;
  delta?: number;
  /** Compact on the canvas (`formatValue`), fuller in the side panel
   *  (`formatNumber`) — same figures, different budget of pixels. */
  format: (n: number) => string;
  /** Show the absolute change alongside `baseline → simulated`. Off on the
   *  canvas, where a 200px node has no room for a fourth number. */
  showDelta?: boolean;
  confidence?: ImpactConfidence;
  /** Spell the confidence out instead of showing its glyph alone. */
  confidenceLabel?: boolean;
}

/**
 * One measure's move: `baseline → simulated`, by how much, and how much of a
 * claim that is.
 *
 * Shared by the canvas node, the lever list and the impact list because they
 * render the same numbers next to each other, and had already drifted into
 * disagreeing about which of them carries the direction colour. A surface that
 * shows only the baseline is the bug this exists to make impossible: pass
 * `simulated`/`delta` and the move renders, everywhere, in one shape.
 */
export function MeasureChange({
  baseline,
  simulated,
  delta,
  format,
  showDelta = false,
  confidence,
  confidenceLabel = false
}: MeasureChangeProps) {
  const signed = (n: number) => `${n > 0 ? "+" : ""}${format(n)}`;
  const mark = confidence ? (
    <ConfidenceMark confidence={confidence} withLabel={confidenceLabel} />
  ) : null;

  if (baseline !== undefined && simulated !== undefined) {
    const up = simulated >= baseline;
    const moved = up ? "text-info" : "text-destructive";
    return (
      <span className='flex flex-wrap items-baseline gap-1.5'>
        <span className='font-mono text-muted-foreground text-xs tabular-nums'>
          {format(baseline)}
        </span>
        <span className='text-muted-foreground'>→</span>
        <span className={cn("font-mono text-xs tabular-nums", moved)}>{format(simulated)}</span>
        {showDelta && delta !== undefined && (
          <span className='font-mono text-[10px] text-muted-foreground tabular-nums'>
            {signed(delta)}
          </span>
        )}
        <span className={cn("font-medium text-[10px] tabular-nums", moved)}>
          {formatPercent(simulated, baseline)}
        </span>
        {mark}
      </span>
    );
  }

  // Delta-only: no time dimension in this layer (hence no baseline), or a
  // signed lever on a measure the baseline could not value. The delta is a
  // real, known number — the unknown case is `unquantifiable`, which never
  // reaches here.
  if (delta !== undefined) {
    return (
      <span className='flex flex-wrap items-baseline gap-1.5'>
        <span className='font-mono text-[9px] text-muted-foreground'>Δ</span>
        <span
          className={cn(
            "font-mono text-xs tabular-nums",
            delta >= 0 ? "text-info" : "text-destructive"
          )}
        >
          {signed(delta)}
        </span>
        {mark}
      </span>
    );
  }

  // Nothing moved it: a lever still parked on its own baseline, or a value with
  // no change resolved against it. Rendering "→ 100 +0.0%" here would dress up
  // a non-event as a forecast.
  if (baseline !== undefined) {
    return (
      <span className='font-mono text-muted-foreground text-xs tabular-nums'>
        {format(baseline)}
      </span>
    );
  }
  return null;
}
