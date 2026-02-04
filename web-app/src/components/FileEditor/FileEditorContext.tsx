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
}

export function FileEditorProvider({
  children,
  pathb64,
  git = false,
  onSaved,
  onChanged
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
    if (isSuccess && fileContent) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
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
    save: async (onSuccess?: () => void) => {
      if (fileState === "saving") return;
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
