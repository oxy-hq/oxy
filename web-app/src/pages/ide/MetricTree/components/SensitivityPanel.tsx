import { Badge } from "@/components/ui/shadcn/badge";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useSensitivity } from "@/hooks/api/useMetricTree";

interface SensitivityPanelProps {
  /** The selected measure, or `null` when nothing is selected. */
  measureId: string | null;
}

/** Ranked drivers of the selected measure. */
export function SensitivityPanel({ measureId }: SensitivityPanelProps) {
  const { data, isLoading, error } = useSensitivity(measureId ?? undefined);

  if (!measureId) {
    return (
      <p className='p-4 text-muted-foreground text-sm'>
        Select a measure in the graph to see what drives it.
      </p>
    );
  }
  if (isLoading) {
    return (
      <div className='flex justify-center p-4'>
        <Spinner />
      </div>
    );
  }
  if (error) {
    return (
      <p className='p-4 text-destructive text-sm'>
        {error instanceof Error ? error.message : "Failed to load drivers."}
      </p>
    );
  }
  if (!data || data.drivers.length === 0) {
    return (
      <p className='p-4 text-muted-foreground text-sm'>
        No drivers found for <span className='font-medium'>{measureId}</span>. It is a leaf metric,
        or no <code>drivers:</code> are declared upstream.
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-2 p-4'>
      <p className='text-muted-foreground text-xs'>
        Drivers of <span className='font-medium text-foreground'>{data.target}</span>, ranked by
        influence.
      </p>
      <ul className='flex flex-col gap-2'>
        {data.drivers.map((driver) => (
          <li key={driver.measure} className='rounded-md border border-border bg-card p-3 text-sm'>
            <div className='flex items-center justify-between gap-2'>
              <span className='font-medium'>{driver.measure}</span>
              <Badge variant={driver.edge_kind === "driver" ? "default" : "secondary"}>
                {driver.edge_kind}
              </Badge>
            </div>
            <div className='mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-muted-foreground text-xs'>
              <span>direction: {driver.direction}</span>
              <span>strength: {driver.strength}</span>
              {driver.effective_coefficient != null && (
                <span>coefficient: {driver.effective_coefficient.toFixed(3)}</span>
              )}
              {driver.lag != null && <span>lag: {driver.lag}d</span>}
            </div>
            {driver.path.length > 1 && (
              <p className='mt-1 text-muted-foreground text-xs'>via {driver.path.join(" → ")}</p>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
