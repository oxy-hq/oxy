import { create } from "zustand";

interface SidebarState {
  state: "open" | "closed";
  toggle: () => void;
  close: () => void;
}

const useSidebarState = create<SidebarState>(set => ({
  state: "closed",
  close: () =>
    set(() => ({
      state: "closed"
    })),
  toggle: () =>
    set(state => ({
      state: state.state === "open" ? "closed" : "open"
    }))
}));

export default useSidebarState;
