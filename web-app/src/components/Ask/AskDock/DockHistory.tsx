import { ThreadHistory } from "@/components/ThreadHistory";
import useAskDock from "@/stores/useAskDock";

/** The history view: a searchable list of the workspace's threads (latest 10).
 *  Selecting one loads it into the in-dock thread view (no navigation). The
 *  dock header already labels the view, so the list's own label is hidden. */
export function DockHistory() {
  return (
    <ThreadHistory
      className='p-3'
      initial={10}
      showLabel={false}
      onSelect={(id) => useAskDock.getState().openThread(id)}
    />
  );
}
