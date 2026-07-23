// Shared in-flight dedup + short result cache for useQuery. Module-level so
// every useQuery across the tree shares one cache. A shared in-flight request
// is intentionally NOT aborted on a single consumer's unmount — others may
// still need it; consumers guard their own setState with a `cancelled` flag.

import { apiErrorFromResponse } from "./errors";

export type QueryResult = { columns: string[]; rows: unknown[][] };
export type Fetcher = (path: string, init: RequestInit) => Promise<Response>;

const SWR_TTL_MS = 30_000;
const inflight = new Map<string, Promise<QueryResult>>();
const cache = new Map<string, { at: number; data: QueryResult }>();

export function queryKey(projectId: string, db: string | undefined, sql: string): string {
  return `${projectId} ${db ?? ""} ${sql}`;
}

export function getCached(
  projectId: string,
  sql: string,
  db: string | undefined
): QueryResult | undefined {
  const e = cache.get(queryKey(projectId, db, sql));
  return e && Date.now() - e.at < SWR_TTL_MS ? e.data : undefined;
}

/** Fetch with in-flight dedup + cache. `force` bypasses the fresh-cache
 *  short-circuit (used by refetch) but still dedupes a concurrent in-flight. */
export async function sharedQuery(
  fetcher: Fetcher,
  projectId: string,
  sql: string,
  db: string | undefined,
  opts: { force?: boolean } = {}
): Promise<QueryResult> {
  const key = queryKey(projectId, db, sql);
  if (!opts.force) {
    const fresh = getCached(projectId, sql, db);
    if (fresh) return fresh;
  }
  const existing = inflight.get(key);
  if (existing) return existing;

  const body = JSON.stringify({ sql, ...(db ? { database: db } : {}) });
  const p = (async () => {
    const resp = await fetcher(`/api/projects/${projectId}/query`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body
    });
    if (!resp.ok) {
      throw await apiErrorFromResponse(resp);
    }
    const data = (await resp.json()) as QueryResult;
    cache.set(key, { at: Date.now(), data });
    return data;
  })().finally(() => inflight.delete(key));

  inflight.set(key, p);
  return p;
}

/** Test-only: reset module state between tests. */
export function __clearQueryCache(): void {
  inflight.clear();
  cache.clear();
}
