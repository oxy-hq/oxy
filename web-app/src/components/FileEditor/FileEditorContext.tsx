import type React from "react";
import { useEffect, useState } from "react";
import useFile from "@/hooks/api/files/useFile";
import useFileGit from "@/hooks/api/files/useFileGit";
import useSaveFile from "@/hooks/api/files/useSaveFile";
import { decodeFilePath } from "@/utils/fileTypes";
import { FileEditorContext } from "./useFileEditorContext";

interface EditorProviderProps {
  children: React.ReactNode;
  pathb64: string;
  git?: boolean;
  onSaved?: (content?: string) => void;
  onChanged?: (content: string) => void;
  /** When provided, replaces the default save-to-current-branch behaviour.
   *  Receives (pathb64, content, onSuccess) and must resolve when the save is complete. */
  onSaveOverride?: (pathb64: string, content: string, onSuccess?: () => void) => Promise<void>;
}

export function FileEditorProvider({
  children,
  pathb64,
  git = false,
  onSaved,
  onChanged,
  onSaveOverride
}: EditorProviderProps) {
  const { mutate: saveFile } = useSaveFile();
  const fileName = decodeFilePath(pathb64);
  const { data: fileContent, isPending, isSuccess } = useFile(pathb64);
  const [fileState, setFileState] = useState<"saved" | "modified" | "saving">("saved");
  const { data: originalContent } = useFileGit(pathb64, git);
  const [showDiff, setShowDiff] = useState(false);
  const [content, setContent] = useState(fileContent || "");

  useEffect(() => {
    onChanged?.(content);
  }, [content, onChanged]);

  useEffect(() => {
    console.log("File content loaded:", { fileContent, isSuccess });
    if (isSuccess && fileContent) {
      setContent(fileContent || "");
    }
  }, [fileContent, isSuccess]);

  const actions = {
    setContent: (newContent: string) => {
      setContent(newContent);
      setFileState("modified");
    },
    setShowDiff: (show: boolean) => {
      setShowDiff(show);
    },
    markSaved: () => {
      setFileState("saved");
    },
    save: async (onSuccess?: () => void) => {
      if (fileState === "saving") return;
      if (onSaveOverride) {
        setFileState("saving");
        try {
          await onSaveOverride(pathb64, content, () => {
            setFileState("saved");
            onSaved?.(content);
            onSuccess?.();
          });
        } catch {
          setFileState("modified");
        }
        return;
      }
      saveFile(
        { pathb64, data: content },
        {
          onSuccess: () => {
            setFileState("saved");
            onSaved?.(content);
            onSuccess?.();
          },
          onError: () => setFileState("modified")
        }
      );
    }
  };

  const contextValue = {
    state: {
      fileName,
      isLoading: isPending,
      content: content || "",
      originalContent,
      fileState,
      showDiff,
      git
    },
    actions: actions
  };

  return <FileEditorContext.Provider value={contextValue}>{children}</FileEditorContext.Provider>;
}
