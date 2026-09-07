import { useSensitivity } from "@/hooks/api/useMetricTree";
import type { DriverStrength } from "@/types/metricTree";
import {
  MagnitudeBar,
  MetaBadge,
  Row,
  SectionHeader,
  SectionSpinner
} from "../../components/semanticGraph";

/** `strength` is a category, not a magnitude — this is only how wide its rank
 *  bar is drawn, so a reader can order drivers without parsing every row. */
const STRENGTH_FRACTION: Record<DriverStrength, number> = {
  strong: 1,
  moderate: 0.6,
  weak: 0.3
};

interface SensitivityPanelProps {
  /** The selected measure, or `null` when nothing is selected. */
  measureId: string | null;
}

/** Ranked drivers of the selected measure. */
export function SensitivityPanel({ measureId }: SensitivityPanelProps) {
  const { data, isLoading, error } = useSensitivity(measureId ?? undefined);

  if (!measureId) {
    return (
      <p className='p-4 text-muted-foreground text-xs'>
        Select a measure in the graph to see what drives it.
      </p>
    );
  }
  if (isLoading) {
    return (
      <div className='p-4'>
        <SectionSpinner label='loading drivers…' />
      </div>
    );
  }
  if (error) {
    return (
      <p className='p-4 text-[11px] text-destructive'>
        {error instanceof Error ? error.message : "Failed to load drivers."}
      </p>
    );
  }
  if (!data || data.drivers.length === 0) {
    return (
      <p className='p-4 text-[11px] text-muted-foreground leading-relaxed'>
        No drivers found for <span className='font-mono text-foreground'>{measureId}</span>. It is a
        leaf metric, or no <span className='font-mono'>drivers:</span> are declared upstream.
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-2 p-4'>
      <SectionHeader title='Drivers' subtitle={data.target} />
      <ul className='flex flex-col gap-1'>
        {data.drivers.map((driver) => (
          <li key={driver.measure}>
            <Row className='flex-col items-stretch gap-1'>
              <div className='flex min-w-0 items-center justify-between gap-2'>
                <span className='min-w-0 truncate text-foreground' title={driver.measure}>
                  {driver.measure}
                </span>
                <MetaBadge>{driver.edge_kind}</MetaBadge>
              </div>
              <MagnitudeBar fraction={STRENGTH_FRACTION[driver.strength] ?? 0} />
              <div className='flex flex-wrap gap-x-2 gap-y-0.5 text-[9.5px] text-muted-foreground'>
                <span>{driver.direction}</span>
                <span>strength {driver.strength}</span>
                {driver.effective_coefficient != null && (
                  <span>coef {driver.effective_coefficient.toFixed(3)}</span>
                )}
                {driver.lag != null && <span>lag {driver.lag}d</span>}
              </div>
              {driver.path.length > 1 && (
                <p className='truncate text-[9.5px] text-muted-foreground'>
                  via {driver.path.join(" → ")}
                </p>
              )}
            </Row>
          </li>
        ))}
      </ul>
    </div>
  );
}
