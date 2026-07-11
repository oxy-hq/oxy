// Shared UI state types for the Traces surface (Theme 3).

/** Status filter in the toolbar. Maps to the API `status` query param. */
export type StatusFilter = "all" | "ok" | "error";

/** List rendering mode: rich cards or a dense table. */
export type TraceView = "card" | "table";

/** UI status → the `status_code` value the backend filters on (undefined = no filter). */
export function statusFilterToApi(status: StatusFilter): string | undefined {
  if (status === "ok") return "Ok";
  if (status === "error") return "Error";
  return undefined;
}
