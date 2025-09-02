import { useCallback } from "react";

export type Viewport = { x: number; y: number; zoom: number };

export const usePersistedViewport = (key: string) => {
  const load = useCallback((): Viewport | null => {
    try {
      const raw = localStorage.getItem(key);
      if (!raw) return null;
      const parsed = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) {
        const p = parsed as Record<string, unknown>;
        if (
          typeof p.x === "number" &&
          typeof p.y === "number" &&
          typeof p.zoom === "number"
        ) {
          return parsed as Viewport;
        }
      }
    } catch {
      /* ignore */
    }
    return null;
  }, [key]);

  const save = useCallback(
    (v: Viewport | undefined) => {
      try {
        if (!v) return;
        localStorage.setItem(key, JSON.stringify(v));
      } catch {
        /* ignore */
      }
    },
    [key],
  );

  return { load, save };
};
