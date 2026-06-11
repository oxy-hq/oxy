import { useEffect, useState } from "react";

/** Debounce a fast-changing value (e.g. a search box) so cross-tenant queries
 *  fire on a pause, not every keystroke. */
export function useDebounced<T>(value: T, delayMs = 300): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(id);
  }, [value, delayMs]);
  return debounced;
}
