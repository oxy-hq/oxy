import { useMonaco, OnMount } from "@monaco-editor/react";
import { useCallback, useEffect, useRef } from "react";
import { monacoGitHubDarkDefaultTheme } from "@/components/FileEditor/hooks/github-dark-theme";
import { FileState } from "@/components/FileEditor";
import useSaveFile from "@/hooks/api/files/useSaveFile";
import { configureMonacoYaml } from "monaco-yaml";
import YamlWorker from "./yaml.worker.js?worker";

window.MonacoEnvironment = {
  getWorker: function (_workerId, label) {
    switch (label) {
      case "yaml":
        return new YamlWorker();
      case "editorWorkerService":
      default:
        return new Worker(
          new URL(
            "monaco-editor/esm/vs/editor/editor.worker.js",
            import.meta.url,
          ),
          { type: "module" },
        );
    }
  },
};

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
  const isConfigured = useRef<boolean>(false);

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
    if (monaco && !isConfigured.current) {
      isConfigured.current = true;

      monaco.editor.defineTheme("github-dark", monacoGitHubDarkDefaultTheme);
      monaco.editor.setTheme("github-dark");

      configureMonacoYaml(monaco, {
        enableSchemaRequest: true,
        hover: true,
        completion: true,
        validate: true,
        format: true,
        schemas: [
          {
            fileMatch: ["**/*.app.yml", "**/*.app.yaml"],
            uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/app.json",
          },
          {
            fileMatch: ["**/*.agent.yml", "**/*.agent.yaml"],
            uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/agent.json",
          },
          {
            fileMatch: ["**/*.workflow.yml", "**/*.workflow.yaml"],
            uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/workflow.json",
          },
          {
            fileMatch: ["**/config.yml", "**/config.yaml"],
            uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/config.json",
          },
        ],
      });

      monaco.editor.addEditorAction({
        id: "save-file",
        label: "Save File",
        keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
        run: () => handleSaveFile(),
      });
    }
  }, [monaco, handleSaveFile]);

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
