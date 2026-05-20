/** How a numeric value is formatted for display in charts + tables. */
export type DisplayFormat = "currency" | "percent" | "number";

export type LineChartDisplay = {
  type: "line_chart" | "line";
  x: string;
  y: string;
  xAxisTitle?: string;
  yAxisTitle?: string;
  data: string;
  series?: string;
  title?: string;
  /** Optional formatting for the y-axis + tooltip values. */
  y_format?: DisplayFormat;
};

export type BarChartDisplay = {
  type: "bar_chart" | "bar";
  data: string;
  x: string;
  y: string;
  title?: string;
  series?: string;
  /** Optional formatting for the y-axis + tooltip values. */
  y_format?: DisplayFormat;
};

export type PieChartDisplay = {
  type: "pie_chart" | "pie";
  data: string;
  name: string;
  value: string;
  title?: string;
  /** Optional formatting for the slice value in the tooltip. */
  value_format?: DisplayFormat;
};

export type TableDisplay = {
  type: "table";
  data: string;
  title?: string;
  /**
   * Optional per-column number formatting. Keys are the output column names
   * in the task result — for semantic_query tasks, that's `<view>__<field>`.
   */
  formats?: Record<string, DisplayFormat>;
};

export type MarkdownDisplay = {
  type: "markdown";
  content: string;
};

type ErrorDisplay = {
  type: "error";
  title: string;
  error: string;
};

type RowDisplay = {
  type: "row";
  /** Number of equal-width columns; defaults to the number of children */
  columns?: number;
  children: Display[];
};

export type Display =
  | ErrorDisplay
  | BarChartDisplay
  | LineChartDisplay
  | TableDisplay
  | PieChartDisplay
  | MarkdownDisplay
  | RowDisplay;

export type TableData = {
  file_path: string;
  json?: string;
};

type Data = string | number | boolean | null | TableData;

export type DataContainer = Data | DataList | DataMap;

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
interface DataList extends Array<DataContainer> {}
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
interface DataMap extends Record<string, DataContainer> {}

export type AppData = {
  data: DataContainer;
  error: string;
};

type ControlType = "select" | "toggle" | "date";

export type ControlConfig = {
  name: string;
  type: ControlType;
  label?: string;
  /** Task name whose first column populates dropdown options */
  source?: string;
  /** Static list of options (used when source is not set) */
  options?: unknown[];
  /** Default value injected on initial load */
  default?: unknown;
};

type AppTaskMode = "client" | "server";

type TaskClientInfo = {
  /** Raw SQL template, may contain Jinja syntax like {{ controls.x }} */
  sql: string;
  /** client = run in DuckDB WASM (default); server = backend round-trip */
  mode: AppTaskMode;
  /** Project-relative files (e.g. "oxymart.csv") the SQL reads. The browser
   *  downloads these once and registers them in DuckDB WASM so the original
   *  SQL runs without modification. */
  source_files?: string[];
};

export type AppDisplay = {
  displays: Display[];
  controls: ControlConfig[];
  /** SQL templates per task name; only execute_sql tasks with inline sql_query */
  tasks: Record<string, TaskClientInfo>;
};

export type AppItem = {
  name: string;
  path: string;
  /** Human-friendly title from the app's `title:` field, when present. */
  title?: string;
  /** Whether the app is published. Unpublished apps are hidden from the sidebar. */
  published?: boolean;
  /** Whether the calling user can change the publish state (false for Viewer). */
  can_publish?: boolean;
};
