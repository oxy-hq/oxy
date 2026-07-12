import { lazy, Suspense } from "react";
import type { EchartsProps } from "./EchartsChart";

// The chart body — and its ~860KB `echarts` runtime import — lives in a
// separate module loaded on demand. Markdown / AppPreview pull `Echarts` into
// the eager app shell, so importing echarts statically here would drag the
// visualization chunk into first paint on every page, chart or not.
const EchartsChart = lazy(() => import("./EchartsChart"));

export const Echarts = (props: EchartsProps) => (
  <Suspense
    fallback={
      <div
        data-testid={props.testId}
        className='chart-wrapper'
        data-chart-index={props.chartIndex ?? 0}
      >
        {props.title && <h2 className='font-bold text-foreground text-xl'>{props.title}</h2>}
        <div style={{ width: "100%", height: "400px" }} />
      </div>
    }
  >
    <EchartsChart {...props} />
  </Suspense>
);
