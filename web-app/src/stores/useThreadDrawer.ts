import { create } from "zustand";

interface ThreadDrawerState {
  /** The thread currently bound to the drawer. Survives `collapse()` so the
   *  edge tab can re-expand it — only `close()` clears it. */
  threadId: string | null;
  /** When true the thread stays bound but the drawer is minimized to the
   *  right-edge tab instead of rendered. */
  collapsed: boolean;
  open: (threadId: string) => void;
  /** Minimize to the edge tab without losing the thread. */
  collapse: () => void;
  /** Re-expand the collapsed drawer. */
  expand: () => void;
  /** Fully dismiss — clears the thread and the edge tab. */
  close: () => void;
}

const useThreadDrawer = create<ThreadDrawerState>()((set) => ({
  threadId: null,
  collapsed: false,
  open: (threadId) => set({ threadId, collapsed: false }),
  collapse: () => set({ collapsed: true }),
  expand: () => set({ collapsed: false }),
  close: () => set({ threadId: null, collapsed: false })
}));

export default useThreadDrawer;
