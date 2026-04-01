import { ErrorBoundary } from "react-error-boundary";
import type { DataContainer, Display } from "@/types/app";
import { BarChart } from "./BarChart";
import { DataTableBlock } from "./DataTableBlock";
import ErrorDisplayBlock from "./ErrorDisplayBlock";
import { LineChart } from "./LineChart";
import { MarkdownDisplayBlock } from "./MarkdownDisplayBlock";
import { PieChart } from "./PieChart";

// DisplayBlock is declared as a function so it can be self-referenced recursively
// inside the "row" case without hoisting issues.
export function DisplayBlock({
  display,
  data,
  idx
}: {
  display: Display;
  data?: DataContainer;
  idx?: number;
}) {
  switch (display.type) {
    case "error":
      return <ErrorDisplayBlock display={display} />;
    case "markdown":
      return <MarkdownDisplayBlock display={display} data={data} />;
    case "line_chart":
    case "line":
      return <LineChart display={display} data={data} index={idx} />;
    case "bar_chart":
    case "bar":
      return <BarChart display={display} data={data} index={idx} />;
    case "table":
      return <DataTableBlock display={display} data={data} />;
    case "pie_chart":
    case "pie":
      return <PieChart display={display} data={data} index={idx} />;
    case "row": {
      const cols = display.columns ?? display.children.length;
      return (
        <div
          style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
          className='grid gap-4'
        >
          {display.children.map((child, childIdx) => (
            <ErrorBoundary
              // biome-ignore lint/suspicious/noArrayIndexKey: display children have no stable id
              key={childIdx}
              resetKeys={[child, data]}
              fallback={
                <ErrorDisplayBlock
                  display={{
                    type: "error",
                    title: "Display Error",
                    error: `Failed to render display of type ${child.type}`
                  }}
                />
              }
            >
              <DisplayBlock display={child} data={data} idx={childIdx} />
            </ErrorBoundary>
          ))}
        </div>
      );
    }
    default:
      return <pre>{JSON.stringify(display)}</pre>;
  }
}

export const Displays = ({ displays, data }: { displays: Display[]; data?: DataContainer }) => (
  <div className='flex flex-col gap-4'>
    {displays.map((display, idx) => (
      <ErrorBoundary
        // biome-ignore lint/suspicious/noArrayIndexKey: display items have no stable id
        key={idx}
        resetKeys={[display, data]}
        fallback={
          <ErrorDisplayBlock
            display={{
              type: "error",
              title: "Display Error",
              error: `Failed to render display of type ${display.type}`
            }}
          />
        }
      >
        <DisplayBlock display={display} data={data} idx={idx} />
      </ErrorBoundary>
    ))}
  </div>
);
