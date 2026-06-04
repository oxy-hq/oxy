import { cn } from "@/libs/utils/cn";

/**
 * Tiny inline SVG sparkline. ~80×24 by default; intended to sit next
 * to a KPI number, not stand alone. No axes, no labels, no legend.
 * The shape and trend ARE the message.
 *
 * Renders a baseline area + a stroke line. Empty / single-point data
 * collapses to a flat line at the vertical midpoint so the tile
 * doesn't shift its layout while the history buffer warms up.
 */
export const Sparkline = ({
  data,
  width = 96,
  height = 28,
  className,
  toneClass = "text-primary"
}: {
  data: number[];
  width?: number;
  height?: number;
  className?: string;
  /** Tailwind class controlling stroke + fill color via `currentColor`. */
  toneClass?: string;
}) => {
  const pad = 2;
  const w = width - pad * 2;
  const h = height - pad * 2;

  if (data.length < 2) {
    // Warm-up state: a calm baseline. Renders the same width / height
    // as the populated state so adjacent layout doesn't reflow.
    return (
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        className={cn(toneClass, "shrink-0 opacity-40", className)}
        aria-hidden
      >
        <line
          x1={pad}
          x2={width - pad}
          y1={height / 2}
          y2={height / 2}
          stroke='currentColor'
          strokeWidth={1}
          strokeDasharray='2 3'
        />
      </svg>
    );
  }

  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;

  const points = data.map((v, i) => {
    const x = pad + (i / (data.length - 1)) * w;
    const y = pad + (1 - (v - min) / range) * h;
    return { x, y };
  });

  const linePath = points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.y}`).join(" ");
  const areaPath = `${linePath} L${points[points.length - 1].x},${height - pad} L${points[0].x},${height - pad} Z`;

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={cn(toneClass, "shrink-0", className)}
      aria-hidden
    >
      <path d={areaPath} fill='currentColor' fillOpacity={0.12} />
      <path
        d={linePath}
        fill='none'
        stroke='currentColor'
        strokeWidth={1.5}
        strokeLinecap='round'
        strokeLinejoin='round'
      />
      <circle
        cx={points[points.length - 1].x}
        cy={points[points.length - 1].y}
        r={1.75}
        fill='currentColor'
      />
    </svg>
  );
};
