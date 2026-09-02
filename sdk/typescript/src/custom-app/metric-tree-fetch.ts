// Shared fetch helpers for the `/api/projects/{id}/semantic/metric-tree*`
// endpoints. Both the metric-tree analysis hooks (`metric-tree-hooks.tsx`)
// and the higher-level World Model node interface (`world-node.tsx`) enter
// the semantic model through these, so the request envelope (`{ v: 1, … }`)
// and error decoding stay identical across the two surfaces.

import { apiErrorFromResponse } from "./errors";
import type { AppFetcher } from "./react";

/** Base path for the metric-tree endpoints of `projectId`. */
export function metricTreePath(projectId: string): string {
  return `/api/projects/${projectId}/semantic/metric-tree`;
}

/** GET `url`, decoding JSON or throwing a typed {@link OxyApiError}. */
export async function getJson<Data>(
  fetcher: AppFetcher,
  url: string,
  signal?: AbortSignal
): Promise<Data> {
  const resp = await fetcher(url, { method: "GET", signal });
  if (!resp.ok) throw await apiErrorFromResponse(resp);
  return (await resp.json()) as Data;
}

/** POST `body` (tagged `v: 1`) to `url`, decoding JSON or throwing. */
export async function postJson<Data>(
  fetcher: AppFetcher,
  url: string,
  body: unknown,
  signal?: AbortSignal
): Promise<Data> {
  const resp = await fetcher(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ v: 1, ...(body as object) }),
    signal
  });
  if (!resp.ok) throw await apiErrorFromResponse(resp);
  return (await resp.json()) as Data;
}
