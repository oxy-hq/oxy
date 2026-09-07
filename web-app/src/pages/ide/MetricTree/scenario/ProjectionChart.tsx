import { useMemo } from "react";
import { Echarts } from "@/components/Echarts";
import { resolveColor } from "@/components/Echarts/resolveColor";
import useTheme, { type ResolvedTheme } from "@/stores/useTheme";
import type { MeasureProjection } from "@/types/metricTree";
import { formatValue } from "./nodeValue";
import { type ProjectionChartColors, projectionChartOptions } from "./projectionChartOptions";
import type { ScenarioCurve } from "./projectionCurves";

interface ProjectionChartProps {
  projection: MeasureProjection;
  curve: ScenarioCurve;
  isLoading: boolean;
}

/**
 * Taller than the ECharts default: the panel is narrow, and a measure that
 * moves a few percent over its own noise has no room to show it in 400px once
 * the zoom slider and legend have taken their share.
 *
 * Exported because the pending placeholder has to reserve exactly this, or the
 * panel jumps when the curves land.
 */
export const PROJECTION_CHART_HEIGHT = "480px";

/**
 * Two curves over one axis: what the measure did, and what it does next with
 * and without the lever.
 *
 * Colours resolve through `resolveColor` because ECharts draws on a canvas and
 * cannot read a CSS variable — and re-resolve when the theme changes, or the
 * chart keeps the previous theme's palette until its data happens to change.
 */
export function ProjectionChart({ projection, curve, isLoading }: ProjectionChartProps) {
  const { theme } = useTheme();

  const colors = useMemo(() => paletteFor(theme), [theme]);
  const options = useMemo(
    () => projectionChartOptions({ projection, curve, colors, format: formatValue }),
    [projection, curve, colors]
  );

  return (
    <Echarts
      options={options}
      isLoading={isLoading}
      height={PROJECTION_CHART_HEIGHT}
      testId='metric-tree-projection-chart'
    />
  );
}

/**
 * The chart palette for the active theme.
 *
 * Takes `theme` even though it reads none of it: `resolveColor` probes the
 * live DOM, so the argument is what makes "the theme changed" a dependency
 * React can see. Without it the memo would hold the previous theme's colours
 * until the data happened to change.
 */
function paletteFor(_theme: ResolvedTheme): ProjectionChartColors {
  return {
    // Three weights, not two. The scenario is the thing being asked about and
    // is the only accent — not emerald, that is reserved for workflow-node
    // success. The baseline forecast is what it gets compared against, so it
    // sits at full foreground: dashed alone did not separate it from the
    // actuals when both were drawn in the same muted grey. The history behind
    // them is context and stays muted.
    actual: resolveColor("--muted-foreground"),
    baseline: resolveColor("--foreground"),
    scenario: resolveColor("--chart-primary"),
    // The band is the *baseline's* interval, so it keeps the baseline's
    // neutral tone — tinting it toward the accent would read as the scenario's
    // uncertainty, which is not what the model stated.
    band: resolveColor("--muted-foreground"),
    axis: resolveColor("--muted-foreground"),
    grid: resolveColor("--border")
  };
}
