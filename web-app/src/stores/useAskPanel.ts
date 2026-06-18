import { create } from "zustand";

export interface AskPrefill {
  message?: string;
  agentPath?: string;
  autoSubmit?: boolean;
}

interface AskPanelState {
  isOpen: boolean;
  prefill: AskPrefill | null;
  open: (prefill?: AskPrefill) => void;
  close: () => void;
  toggle: () => void;
}

const useAskPanel = create<AskPanelState>()((set) => ({
  isOpen: false,
  prefill: null,
  open: (prefill) => set({ isOpen: true, prefill: prefill ?? null }),
  close: () => set({ isOpen: false, prefill: null }),
  toggle: () =>
    set((s) => (s.isOpen ? { isOpen: false, prefill: null } : { isOpen: true, prefill: null }))
}));

export default useAskPanel;
