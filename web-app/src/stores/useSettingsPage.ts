import { create } from "zustand";

interface SettingsState {
  isOpen: boolean;
  setIsOpen: (isOpen: boolean) => void;
}

const useSettings = create<SettingsState>()((set) => ({
  isOpen: false,
  setIsOpen: (isOpen: boolean) => set({ isOpen }),
}));

export default useSettings;
