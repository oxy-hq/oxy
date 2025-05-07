import { DataContainer, Display } from "@/types/app";

// Import refactored display blocks
import { MarkdownDisplayBlock } from "./MarkdownDisplayBlock";
import { LineChart } from "./LineChart";
import { BarChart } from "./BarChart";
import { PieChart } from "./PieChart";
import { DataTableBlock } from "./DataTableBlock";

// Display block switch
const DisplayBlock = ({
  display,
  data,
}: {
  display: Display;
  data: DataContainer;
}) => {
  switch (display.type) {
    case "markdown":
      return <MarkdownDisplayBlock display={display} data={data} />;
    case "line_chart":
      return <LineChart display={display} data={data} />;
    case "bar_chart":
      return <BarChart display={display} data={data} />;
    case "table":
      return <DataTableBlock display={display} data={data} />;
    case "pie_chart":
      return <PieChart display={display} data={data} />;
    default:
      return <pre>{JSON.stringify(display)}</pre>;
  }
};

// Displays list
export const Displays = ({
  displays,
  data,
}: {
  displays: Display[];
  data: DataContainer;
}) => (
  <div className="flex flex-col gap-4">
    {displays.map((display, idx) => (
      <DisplayBlock key={idx} display={display} data={data} />
    ))}
  </div>
);
