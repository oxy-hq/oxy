import { useResizableDock } from "@/hooks/useResizableDock";

/**
 * Width + left-edge resize for the docked custom app.
 *
 * Same interaction as the Ask dock, deliberately different envelope: an app
 * pane is the thing the user came to look at, so it defaults to two-thirds of
 * the viewport and may grow to 92% — leaving a sliver of shell rather than a
 * balanced split. The Ask dock caps at 60% because there the *page* is the
 * subject and the dock assists it. Same hook, opposite priority.
 */
export function useAppDockWidth() {
  return useResizableDock({
    storageKey: "oxy:app-dock-width",
    defaultWidth: Math.round(window.innerWidth * 0.66),
    minWidth: 480,
    maxFraction: 0.92
  });
}
