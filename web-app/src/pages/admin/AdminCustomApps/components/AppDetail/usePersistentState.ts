import { useCallback, useState } from "react";

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
): [T, (next: T) => void] {
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

  const set = useCallback(
    (next: T) => {
      setValue(next);
      try {
        window.localStorage.setItem(key, JSON.stringify(next));
      } catch (err) {
        // Storage blocked or full (private browsing, quota). The choice still
        // applies for this session — it just won't survive a reload.
        console.warn(`Could not persist "${key}" preference`, err);
      }
    },
    [key]
  );

  return [value, set];
}
