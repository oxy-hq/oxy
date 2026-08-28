import { useCallback, useEffect, useRef, useState } from "react";

export interface ResizableDockOptions {
  /** `localStorage` key the user's chosen width persists under. */
  storageKey: string;
  /** Width used before the user has ever resized this dock. */
  defaultWidth: number;
  /** Floor, in px. The dock never gets narrower than this on desktop. */
  minWidth: number;
  /**
   * Ceiling, as a fraction of the viewport. What is left over is what the
   * compacted `<main>` gets, so this is really "how much of the page may the
   * dock take" — an Ask panel and a full app pane want very different answers.
   */
  maxFraction: number;
  /** Px moved per arrow-key press on the resize handle. */
  step?: number;
}

/**
 * `localStorage` access that cannot take the page down.
 *
 * Private-mode Safari and storage-blocked browsers throw on the *accessor*, not
 * just on write — and this one runs inside a `useState` initialiser, so an
 * unguarded read throws during render and takes the dock down with it. Hence
 * the two helpers rather than a comment saying "be careful".
 */
function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* a forgotten width is not worth an error boundary */
  }
}

/** Below sm (640px) a dock is full-width and not resizable. */
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
 * Width + left-edge resize for a right-hand dock that COMPACTS the page rather
 * than floating over it.
 *
 * The drag math (`innerWidth - clientX`) holds because every dock built on this
 * is flush with the viewport's right edge and puts its handle on the left edge.
 * A dock that ever floats, or docks left, needs different math — not a new
 * option on this one.
 *
 * Extracted from the Ask dock when the app dock arrived wanting the same
 * behaviour with a very different size envelope (an Ask panel is a sidebar; an
 * app pane is the page). The parameters are exactly the axes on which the two
 * differ; everything else is shared because it is the same interaction.
 */
export function useResizableDock({
  storageKey,
  defaultWidth,
  minWidth,
  maxFraction,
  step = 24
}: ResizableDockOptions) {
  const isDesktop = useIsDesktop();

  const maxWidth = useCallback(() => Math.round(window.innerWidth * maxFraction), [maxFraction]);
  const clampWidth = useCallback(
    (w: number) => Math.min(Math.max(w, minWidth), maxWidth()),
    [minWidth, maxWidth]
  );

  const [width, setWidth] = useState(() => {
    const stored = Number(readStored(storageKey));
    return clampWidth(Number.isFinite(stored) && stored > 0 ? stored : defaultWidth);
  });
  const [dragging, setDragging] = useState(false);
  const widthRef = useRef(width);
  widthRef.current = width;

  // Re-clamp on viewport resize so a dock sized wide on a big monitor can't
  // exceed its cap after the window shrinks. Deliberately NOT persisted — the
  // stored value is the user's explicit choice and a transient small window
  // should not overwrite it.
  useEffect(() => {
    const onResize = () => setWidth((w) => clampWidth(w));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [clampWidth]);

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
    [dragging, clampWidth]
  );
  const onPointerUp = useCallback(() => {
    setDragging(false);
    writeStored(storageKey, String(widthRef.current));
  }, [storageKey]);
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      e.preventDefault();
      // Left edge: arrowing left widens the dock.
      const next = clampWidth(widthRef.current + (e.key === "ArrowLeft" ? step : -step));
      setWidth(next);
      writeStored(storageKey, String(next));
    },
    [clampWidth, step, storageKey]
  );

  return {
    width,
    isDesktop,
    dragging,
    minWidth,
    maxWidth: maxWidth(),
    handleProps: { onPointerDown, onPointerMove, onPointerUp, onKeyDown }
  };
}
