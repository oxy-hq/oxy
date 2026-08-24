import { useResizableDock } from "@/hooks/useResizableDock";

/**
 * Width + left-edge resize for the docked Ask panel.
 *
 * The behaviour lives in `useResizableDock`, shared with the app dock; what is
 * local to Ask is the size envelope — a sidebar next to the page, capped at
 * 60vw so the compacted `<main>` keeps at least 40vw.
 */
export function useDockWidth() {
  return useResizableDock({
    // The old ThreadDrawer key, so a returning user keeps their width
    // (re-clamped to the current cap on read).
    storageKey: "oxy:thread-drawer-width",
    defaultWidth: 480,
    minWidth: 360,
    maxFraction: 0.6
  });
}
