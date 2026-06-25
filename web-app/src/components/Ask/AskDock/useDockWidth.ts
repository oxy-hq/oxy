import { useCallback, useEffect, useRef, useState } from "react";

// Reuse the old ThreadDrawer key so a returning user keeps their width
// (re-clamped to the new cap on read).
const WIDTH_KEY = "oxy:thread-drawer-width";
const DEFAULT_WIDTH = 480;
const MIN_WIDTH = 360;
// Cap the dock at 60vw so the compacted <main> keeps at least 40vw.
const MAX_FRACTION = 0.6;
const STEP = 24;

const maxWidth = () => Math.round(window.innerWidth * MAX_FRACTION);
const clampWidth = (w: number) => Math.min(Math.max(w, MIN_WIDTH), maxWidth());

const loadWidth = () => {
  const stored = Number(localStorage.getItem(WIDTH_KEY));
  return clampWidth(Number.isFinite(stored) && stored > 0 ? stored : DEFAULT_WIDTH);
};

/** Below sm (640px) the dock is full-width and not resizable. */
function useIsDesktop() {
  const [v, setV] = useState(() => window.matchMedia("(min-width: 640px)").matches);
  useEffect(() => {
    const m = window.matchMedia("(min-width: 640px)");
    const fn = (e: MediaQueryListEvent) => setV(e.matches);
    m.addEventListener("change", fn);
    return () => m.removeEventListener("change", fn);
  }, []);
  return v;
}

/**
 * Width + left-edge resize for the docked Ask panel — lifted from the old
 * ThreadDrawer. The drag math `innerWidth - clientX` holds because the dock's
 * right edge is the viewport's right edge (the handle sits on its left edge).
 */
export function useDockWidth() {
  const isDesktop = useIsDesktop();
  const [width, setWidth] = useState(loadWidth);
  const [dragging, setDragging] = useState(false);
  const widthRef = useRef(width);
  widthRef.current = width;

  // Re-clamp on viewport resize so a dock sized wide can't exceed the 60vw
  // cap (keeping <main> ≥ 40vw) after the window shrinks. We don't persist
  // here — localStorage keeps the user's explicit drag/arrow width.
  useEffect(() => {
    const onResize = () => setWidth((w) => clampWidth(w));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDragging(true);
  }, []);
  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return;
      setWidth(clampWidth(window.innerWidth - e.clientX));
    },
    [dragging]
  );
  const onPointerUp = useCallback(() => {
    setDragging(false);
    localStorage.setItem(WIDTH_KEY, String(widthRef.current));
  }, []);
  const onKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    // Left edge: dragging/arrowing left widens the dock.
    const next = clampWidth(widthRef.current + (e.key === "ArrowLeft" ? STEP : -STEP));
    setWidth(next);
    localStorage.setItem(WIDTH_KEY, String(next));
  }, []);

  return {
    width,
    isDesktop,
    dragging,
    minWidth: MIN_WIDTH,
    maxWidth: maxWidth(),
    handleProps: { onPointerDown, onPointerMove, onPointerUp, onKeyDown }
  };
}
