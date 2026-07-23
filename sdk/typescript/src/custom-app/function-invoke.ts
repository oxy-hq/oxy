// In-flight deduplication for `useFunction().invoke()`.
//
// A function is arbitrary server-side logic and is frequently SIDE-EFFECTFUL
// (warehouse writes, external POSTs, ELT kick-offs), so we must NOT cache
// completed results — a fresh invoke after the first settles has to run again.
//
// What IS safe (and desirable) is collapsing *concurrent* identical invokes
// into ONE request: a double-click, or two components invoking the same
// function with the same body at the same time, should not fire two POSTs
// (which would, e.g., post a journal entry twice). Once the in-flight request
// settles, the entry is dropped, so the next invoke runs fresh.
//
// Result *caching* is a separate, opt-in, server-side feature (a function
// declares `cache: { ttlSeconds }` in oxy-app.json) — never a client default.

const inflight = new Map<string, Promise<unknown>>();

/**
 * Dedup key for an invocation: function name + its (stable-serialized) body,
 * joined by a newline. Function names are `[a-z][a-z0-9-]*` (no newline), so
 * the separator can never collide with a name.
 */
export function functionInvokeKey(name: string, body: unknown): string {
  return `${name}\n${JSON.stringify(body ?? {})}`;
}

/**
 * Run `run()` unless an identical invocation is already in flight, in which
 * case share its promise. The entry is removed once the promise settles — so
 * this dedups concurrency only, it does NOT memoize the result.
 */
export function sharedFunctionInvoke<Data>(key: string, run: () => Promise<Data>): Promise<Data> {
  const existing = inflight.get(key) as Promise<Data> | undefined;
  if (existing) return existing;
  const p = run().finally(() => {
    inflight.delete(key);
  });
  inflight.set(key, p);
  return p;
}

/** Test-only: reset in-flight state between tests. */
export function __clearInflightFunctions(): void {
  inflight.clear();
}
