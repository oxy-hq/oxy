import { Spinner } from "@/components/ui/shadcn/spinner";
import type { MeasureProjection } from "@/types/metricTree";
import { PROJECTION_CHART_HEIGHT, ProjectionChart } from "./ProjectionChart";
import { SCENARIO_REFUSAL_TEXT, type scenarioCurve } from "./projectionCurves";

interface ProjectionBodyProps {
  note: string | null;
  projection: MeasureProjection | undefined;
  curve: ReturnType<typeof scenarioCurve> | null;
  isFetching: boolean;
}

/** Everything under the controls: the chart, or what is standing in for it. */
export function ProjectionBody({ note, projection, curve, isFetching }: ProjectionBodyProps) {
  return (
    <>
      {projection && curve ? (
        <>
          <ProjectionChart projection={projection} curve={curve} isLoading={isFetching} />
          {/* A refusal is reported, never implied. Silence and "the model
              declined" look identical on a chart with one line on it. */}
          {curve.kind === "refused" && (
            <div
              className='text-[11px] text-muted-foreground leading-relaxed'
              data-testid='projection-refusal'
            >
              {SCENARIO_REFUSAL_TEXT[curve.reason]}
            </div>
          )}
        </>
      ) : isFetching ? (
        <Pending />
      ) : null}
      {note && (
        <div
          className='text-[11px] text-muted-foreground leading-relaxed'
          data-testid='projection-note'
        >
          {note}
        </div>
      )}
    </>
  );
}

/**
 * The chart's footprint while there is no chart yet.
 *
 * [`ProjectionChart`] carries its own spinner, but ECharts can only draw one
 * over a chart that already exists — and there is no chart on the first
 * request, nor on any request that changes the query key, which is every change
 * of measure, bucket, horizon or model. Those all landed as an empty panel with
 * nothing moving in it, indistinguishable from a projection that had quietly
 * failed to ask for anything.
 *
 * Reserving the chart's own height is the other half: a placeholder shorter
 * than what replaces it just moves the jump rather than removing it.
 */
function Pending() {
  return (
    <div
      className='flex w-full items-center justify-center gap-2 text-[11px] text-muted-foreground'
      style={{ height: PROJECTION_CHART_HEIGHT }}
      data-testid='projection-pending'
    >
      <Spinner className='size-3.5' />
      running the projection
    </div>
  );
}
