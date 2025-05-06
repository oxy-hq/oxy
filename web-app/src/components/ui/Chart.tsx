/* eslint-disable @typescript-eslint/no-explicit-any */
"use client";

import { VegaLite } from "react-vega";
import { TopLevelSpec } from "vega-lite";
import { useResizeDetector } from "react-resize-detector";
import { useMemo } from "react";

export interface IChartProps {
  spec: TopLevelSpec;
  aspectRatio?: number;
}

export default function Chart(props: IChartProps) {
  const { ref: containerRef, width } = useResizeDetector();
  const { spec, aspectRatio = 16 / 9 } = props;

  // Calculate height based on aspect ratio
  const height = useMemo(() => {
    return width ? width / aspectRatio : undefined;
  }, [width, aspectRatio]);

  // Enhanced color palette for dark theme - more vibrant and distinct colors
  const darkColors = [
    "#3aa7ff",
    "#50c878",
    "#ffc857",
    "#ff6b6b",
    "#bf9af7",
    "#64dfdf",
    "#ff9f7f",
    "#a0e8af",
  ];

  // Enhanced spec with better defaults for readability
  const enhancedSpec: TopLevelSpec = {
    ...spec,
    width: "container",
    height: "container",
    padding: { left: 60, right: 20, top: 30, bottom: 40 }, // Increased left padding for axis labels
    background: "transparent",
    config: {
      ...(spec.config || {}),
      autosize: { type: "fit", contains: "padding" },
      font: "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
      background: "transparent",

      // Better mark configuration specifically for line charts
      line: {
        point: true, // Always show points for better readability
        strokeWidth: 2.5, // Slightly thinner than 3 for clarity
        strokeCap: "round", // Rounded line ends
        opacity: 0.9, // Slight transparency for overlapping lines
        ...((spec.config as any)?.line || {}),
      },

      // Point configuration for better visibility
      point: {
        filled: true,
        size: 50, // Slightly smaller than 60 for less visual noise
        strokeWidth: 1.5, // Thinner stroke for cleaner look
        ...((spec.config as any)?.point || {}),
      },

      // Axis improvements for better readability
      axis: {
        labelFont: "'Inter', sans-serif",
        labelFontSize: 13, // Increased from 12
        labelFontWeight: 500,
        labelPadding: 10, // Increased from 8
        labelColor: "rgba(255,255,255,0.9)", // Slightly transparent for less harshness
        titleFont: "'Inter', sans-serif",
        titleFontSize: 14,
        titleFontWeight: 600,
        titlePadding: 15, // Increased from 12
        titleColor: "#ffffff",
        gridColor: "rgba(255,255,255,0.08)", // More subtle grid
        gridDash: [2, 2], // Dashed grid lines for better distinction from data
        gridOpacity: 1,
        gridWidth: 1,
        ticks: false,
        domain: false,
        labelLimit: 180, // Increased from default
        labelOverlap: true,
        ...((spec.config as any)?.axis || {}),
      },

      // X-axis specific improvements
      axisX: {
        labelAngle: 0,
        labelAlign: "center",
        labelBaseline: "top",
        labelPadding: 8,
        ...((spec.config as any)?.axisX || {}),
      },

      // Y-axis specific improvements
      axisY: {
        labelAlign: "right",
        labelPadding: 10,
        labelBaseline: "middle",
        grid: true,
        ...((spec.config as any)?.axisY || {}),
      },

      // Better tick configuration
      axisQuantitative: {
        tickCount: 5, // Show 5 ticks for better readability
        ...((spec.config as any)?.axisQuantitative || {}),
      },

      // Improved legend for better readability
      legend: {
        orient: "top",
        direction: "horizontal",
        symbolType: "circle",
        symbolSize: 80,
        symbolFillOpacity: 0.9, // Slight transparency
        symbolStrokeWidth: 1.5, // Thinner stroke
        titleFont: "'Inter', sans-serif",
        titleFontSize: 14,
        titleFontWeight: 600,
        labelFont: "'Inter', sans-serif",
        labelFontSize: 13, // Increased from 12
        labelFontWeight: 500,
        labelColor: "rgba(255,255,255,0.9)", // Slightly transparent
        titleColor: "#ffffff",
        titlePadding: 10,
        labelPadding: 5,
        padding: 15, // Increased padding
        offset: 5, // Offset from chart
        ...((spec.config as any)?.legend || {}),
      },

      // Apply enhanced color scheme
      range: {
        category: darkColors,
        ...((spec.config as any)?.range || {}),
      },

      // Improved view
      view: {
        strokeWidth: 0,
        ...((spec.config as any)?.view || {}),
      },
    },
  };

  // Add default tooltip encoding if none exists
  if ("encoding" in spec && spec.encoding && !spec.encoding.tooltip) {
    const xField = (spec.encoding.x as any)?.field;
    const yField = (spec.encoding.y as any)?.field;

    if (xField && yField) {
      (enhancedSpec as any).encoding = {
        ...(enhancedSpec as any).encoding,
        tooltip: [
          { field: xField, type: (spec.encoding.x as any)?.type || "nominal" },
          {
            field: yField,
            type: (spec.encoding.y as any)?.type || "quantitative",
          },
        ],
      };
    }
  }

  // Key for forcing re-render when container size changes
  const key = useMemo(() => Date.now().toString(), []);

  return (
    <div
      ref={containerRef}
      className="w-full h-full"
      style={{
        position: "relative",
        height: height || "100%",
      }}
    >
      <VegaLite
        renderer="svg"
        key={key}
        spec={enhancedSpec}
        className="w-full h-full absolute"
        actions={false}
      />
    </div>
  );
}
