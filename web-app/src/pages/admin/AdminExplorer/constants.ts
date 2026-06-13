/** Resource tabs for the cross-tenant explorer. */
export type Resource = "threads" | "runs";

export const RESOURCES: { id: Resource; label: string }[] = [
  { id: "threads", label: "Threads" },
  { id: "runs", label: "Runs" }
];

export const PAGE_SIZE = 25;

/** Sentinel value for "no filter" — Radix `Select.Item` rejects "". */
export const ALL = "all";

export interface FilterOption {
  value: string;
  label: string;
}

/** Threads have no `task_status`; the filter maps onto `is_processing`. */
export const THREAD_STATUSES: FilterOption[] = [
  { value: ALL, label: "All statuses" },
  { value: "live", label: "Live" },
  { value: "done", label: "Completed" }
];

/** Known `agentic_runs.task_status` values, including transient FSM states. */
export const RUN_STATUSES: FilterOption[] = [
  { value: ALL, label: "All statuses" },
  { value: "running", label: "Running" },
  { value: "delegating", label: "Delegating" },
  { value: "awaiting_input", label: "Awaiting input" },
  { value: "needs_resume", label: "Needs resume" },
  { value: "done", label: "Done" },
  { value: "failed", label: "Failed" },
  { value: "dead", label: "Dead" },
  { value: "cancelled", label: "Cancelled" }
];

/** Known `threads.source_type` values. */
export const THREAD_SOURCE_TYPES: FilterOption[] = [
  { value: ALL, label: "All sources" },
  { value: "analytics", label: "Analytics" },
  { value: "agent", label: "Agent" }
];

/** Known `agentic_runs.source_type` values. */
export const RUN_SOURCE_TYPES: FilterOption[] = [
  { value: ALL, label: "All sources" },
  { value: "analytics", label: "Analytics" },
  { value: "builder", label: "Builder" },
  { value: "workflow", label: "Workflow" },
  { value: "airway", label: "Airway" }
];

/** Resolve a UI filter value to the query param, collapsing the "all" sentinel to undefined. */
export const filterValue = (value: string): string | undefined =>
  value === ALL ? undefined : value;
