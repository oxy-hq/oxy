export type LineChartDisplay = {
  type: "line_chart";
  x: string;
  y: string;
  xAxisTitle?: string;
  yAxisTitle?: string;
  data: string;
  series?: string;
  title?: string;
};

export type BarChartDisplay = {
  type: "bar_chart";
  data: string;
  x: string;
  y: string;
  title?: string;
  series?: string;
};

export type PieChartDisplay = {
  type: "pie_chart";
  data: string;
  name: string;
  value: string;
  title?: string;
};

export type TableDisplay = {
  type: "table";
  data: string;
  title?: string;
};

export type MarkdownDisplay = {
  type: "markdown";
  content: string;
};

export type ErrorDisplay = {
  type: "error";
  title: string;
  error: string;
};

export type Display =
  | ErrorDisplay
  | BarChartDisplay
  | LineChartDisplay
  | TableDisplay
  | PieChartDisplay
  | MarkdownDisplay;

export type TableData = {
  file_path: string;
};

export type Data = string | number | boolean | null | TableData;

export type DataContainer = Data | DataList | DataMap;

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface DataList extends Array<DataContainer> {}
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface DataMap extends Record<string, DataContainer> {}

export type AppData = {
  data: DataContainer;
  error: string;
};

export type AppDisplay = {
  displays: Display[];
};

export type AppItem = {
  name: string;
  path: string;
};

export type Chunk = {
  content: string;
  file_path: string;
  is_error: boolean;
  step: string;
  input_tokens: number;
  output_tokens: number;
};
