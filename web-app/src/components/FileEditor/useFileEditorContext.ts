import React, { useContext } from "react";
import type { FileState } from ".";

export interface EditorContextActions {
  setContent: (content: string) => void;
  setShowDiff: (show: boolean) => void;
  save: (onSuccess?: () => void) => Promise<void>;
}

export interface EditorContextState {
  content: string;
  originalContent?: string;
  fileState: FileState;
  showDiff: boolean;
  isLoading: boolean;
  fileName: string;
  git?: boolean;
}

export interface EditorContextValue {
  state: EditorContextState;
  actions: EditorContextActions;
}

export const FileEditorContext = React.createContext<EditorContextValue | null>(null);

export function useFileEditorContext(): EditorContextValue {
  const context = useContext(FileEditorContext);
  if (!context) {
    throw new Error("useEditorContext must be used within an EditorProvider");
  }
  return context as EditorContextValue;
}
