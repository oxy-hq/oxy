import { create } from "zustand";

interface ManageWorkspacesDialogState {
  isOpen: boolean;
  open: () => void;
  close: () => void;
}

/**
 * Global open/close state for the Manage Workspaces dialog so the dialog can
 * be rendered at the top of WorkspaceLayout — outside any popover or sidebar
 * sheet — and won't fight Radix focus management when triggered from a
 * dropdown/popover.
 */
const useManageWorkspacesDialog = create<ManageWorkspacesDialogState>((set) => ({
  isOpen: false,
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false })
}));

export default useManageWorkspacesDialog;
