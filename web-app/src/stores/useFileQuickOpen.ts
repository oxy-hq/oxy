import { create } from "zustand";

interface FileQuickOpenState {
  isOpen: boolean;
  setIsOpen: (isOpen: boolean) => void;
}

const useFileQuickOpen = create<FileQuickOpenState>()((set) => ({
  isOpen: false,
  setIsOpen: (isOpen: boolean) => set({ isOpen })
}));

export default useFileQuickOpen;
