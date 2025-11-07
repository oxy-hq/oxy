import { useMonaco, OnMount } from "@monaco-editor/react";
import { useCallback, useEffect, useRef } from "react";
import { FileState } from "@/components/FileEditor";
import useSaveFile from "@/hooks/api/files/useSaveFile";
import { configureMonaco, configureMonacoEnvironment } from "../monacoConfig";

configureMonacoEnvironment();

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
  const isConfigured = useRef<boolean>(false);
  const saveHandlerRef = useRef<((afterSave?: () => void) => void) | null>(
    null,
  );

  const handleSaveFile = useCallback(
    (afterSave?: () => void) => {
      if (fileState === "saving") return;
      if (!monaco) return;

      const editor = monaco.editor;
      if (!editor) return;

      const model = editor.getModels()[0];
      if (!model) return;

      const data = model.getValue();
      if (data === null || data === undefined) return;

      onFileStateChange("saving");

      saveFile(
        { pathb64, data },
        {
          onSuccess: () => {
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
    saveHandlerRef.current = handleSaveFile;
  }, [handleSaveFile]);

  useEffect(() => {
    if (monaco && !isConfigured.current) {
      isConfigured.current = true;
      configureMonaco(monaco);

      monaco.editor.addEditorAction({
        id: "save-file",
        label: "Save File",
        keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
        run: () => saveHandlerRef.current?.(),
      });
    }
  }, [monaco]);

  const handleEditorMount: OnMount = useCallback(() => {
    // Editor mounted successfully
  }, []);

  return {
    monaco,
    handleEditorMount,
    handleSaveFile,
  };
};

export default useMonacoEditor;
