import { useMonaco, OnMount } from "@monaco-editor/react";
import { useCallback, useEffect, useRef } from "react";
import { monacoGitHubDarkDefaultTheme } from "@/components/FileEditor/hooks/github-dark-theme";
import { FileState } from "@/components/FileEditor";
import useSaveFile from "@/hooks/api/useSaveFile";

interface UseMonacoEditorProps {
  fileState: FileState;
  pathb64: string;
  onFileStateChange: (state: FileState) => void;
  onSaved?: () => void;
}

const useMonacoEditor = ({
  onFileStateChange,
  fileState,
  pathb64,
  onSaved,
}: UseMonacoEditorProps) => {
  const { mutate: saveFile } = useSaveFile();

  const monaco = useMonaco();
  const lastSavedVersionId = useRef<number | null>(null);

  const handleSaveFile = useCallback(
    (afterSave?: () => void) => {
      if (fileState === "saving") return;
      if (!monaco) return;
      const editor = monaco.editor;
      if (!editor) return;
      const model = editor.getModels()[0];
      if (!model) return;
      const data = model.getValue();
      if (!data && data !== "") return;

      saveFile(
        { pathb64: pathb64 ?? "", data },
        {
          onSuccess: () => {
            lastSavedVersionId.current = model.getAlternativeVersionId();
            onFileStateChange("saved");
            onSaved?.();
            afterSave?.();
          },
          onError: () => onFileStateChange("modified"),
        },
      );
    },
    [fileState, pathb64, monaco, saveFile, onFileStateChange, onSaved],
  );

  useEffect(() => {
    if (monaco) {
      monaco.editor.defineTheme("github-dark", monacoGitHubDarkDefaultTheme);
      monaco.editor.setTheme("github-dark");
      monaco.editor.addEditorAction({
        id: "save-file",
        label: "Save File",
        keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
        run: () => handleSaveFile(),
      });
    }
  }, [handleSaveFile, monaco]);

  const handleEditorMount: OnMount = (editor) => {
    const model = editor.getModel();
    if (model) {
      lastSavedVersionId.current = model.getAlternativeVersionId();

      model.onDidChangeContent(() => {
        const currentVersionId = model.getAlternativeVersionId();
        onFileStateChange(
          lastSavedVersionId.current !== currentVersionId
            ? "modified"
            : "saved",
        );
      });
    }
  };

  return {
    monaco,
    lastSavedVersionId,
    handleEditorMount,
    handleSaveFile,
  };
};

export default useMonacoEditor;
