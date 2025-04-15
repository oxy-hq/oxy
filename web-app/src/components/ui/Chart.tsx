"use client";

import { VegaLite } from "react-vega";
import { TopLevelSpec } from "vega-lite";
import { useResizeDetector } from "react-resize-detector";
import { useMemo } from "react";

export interface IChartProps {
  spec: TopLevelSpec;
}

export default function Chart(props: IChartProps) {
  const { ref: containerRef } = useResizeDetector();

  const spec: TopLevelSpec = {
    ...props.spec,
    width: "container",
    height: "container",
    center: true,
    config: {
      autosize: { contains: "padding", type: "fit" },
      font: "'Inter', sans-serif",
      background: "transparent",
      mark: { tooltip: true },
      axis: {
        labelFontSize: 14,
        labelFontWeight: 400,
        labelFont: "'Inter', sans-serif",
        labelFontStyle: "normal",
        labelLimit: 180,
        labelPadding: 12,
        labelSeparation: 4,
        labelOverlap: true,
        labelAngle: 0,
        titleFontWeight: 400,
        titleFont: "'Inter', sans-serif",
        titleFontSize: 14,
        titleFontStyle: "normal",
        titlePadding: 20,
        ticks: false,
        grid: true,
        domain: false,
      },
      axisQuantitative: {
        tickCount: 3,
      },
      legend: {
        symbolSize: 260,
        symbolType:
          "M -1 -0.5 A 0.5 0.5 0 0 1 -0.5 -1 h 1 A 0.5 0.5 0 0 1 1 -0.5 v 1 A 0.5 0.5 0 0 1 0.5 1 h -1 A 0.5 0.5 0 0 1 -1 0.5 v -1 Z",

        titleFont: "'Inter', sans-serif",
        titleFontWeight: 400,
        titleFontSize: 14,
        titleFontStyle: "normal",
        titlePadding: 12,
        labelFont: "'Inter', sans-serif",
        labelFontWeight: 400,
        labelFontSize: 14,
        labelFontStyle: "normal",
        labelPadding: 4,
      },
      view: { strokeWidth: 1, stroke: "var(--colors-border-primary)" },
    },
  };

  const key = useMemo(() => {
    return Date.now();
  }, []);

  return (
    <div ref={containerRef} className="w-full h-full relative">
      <VegaLite
        renderer="svg"
        key={key}
        spec={spec}
        className="w-full h-full absolute"
        actions={false}
      />
    </div>
  );
}
