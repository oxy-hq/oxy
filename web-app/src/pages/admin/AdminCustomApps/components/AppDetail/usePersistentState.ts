import { useCallback, useEffect, useRef, useState } from "react";

/**
 * `useState` whose value survives a reload, stored as JSON under `key`.
 *
 * Deliberately not a Zustand store: these are per-operator layout preferences
 * with no cross-component readers, so `localStorage` is the whole requirement.
 *
 * `revive` validates whatever came back out of storage — a stale key from an
 * older build, or a hand-edited value, falls back to `fallback` instead of
 * rendering a broken layout.
 */
export function usePersistentState<T>(
  key: string,
  fallback: T,
  revive: (raw: unknown) => T | null
): [T, (next: T | ((prev: T) => T)) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = window.localStorage.getItem(key);
      if (raw === null) return fallback;
      return revive(JSON.parse(raw)) ?? fallback;
    } catch (err) {
      console.warn(`Ignoring unreadable "${key}" preference`, err);
      return fallback;
    }
  });

  // Accepts an updater, like `useState`, so a caller that derives the next value
  // from the current one does not have to close over it — a stale closure here
  // silently drops a concurrent change, and the collapse map has several
  // independent writers.
  //
  // The write is an effect, not something the updater does. An updater is
  // re-invoked under StrictMode and may be recomputed and discarded under
  // concurrent rendering, so persisting from inside one stores values that
  // never render.
  const set = useCallback((next: T | ((prev: T) => T)) => {
    setValue((prev) => (typeof next === "function" ? (next as (p: T) => T)(prev) : next));
  }, []);

  const initial = useRef(true);
  useEffect(() => {
    // Skip the mount pass: it would write back exactly what was just read, and
    // on a storage-blocked browser it would warn on every mount for nothing.
    if (initial.current) {
      initial.current = false;
      return;
    }
    try {
      window.localStorage.setItem(key, JSON.stringify(value));
    } catch (err) {
      // Storage blocked or full (private browsing, quota). The choice still
      // applies for this session — it just won't survive a reload.
      console.warn(`Could not persist "${key}" preference`, err);
    }
  }, [key, value]);

  return [value, set];
}
