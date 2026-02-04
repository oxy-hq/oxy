import { useMonaco } from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import { configureMonaco, configureMonacoEnvironment } from "../monacoConfig";

configureMonacoEnvironment();

interface UseMonacoEditorProps {
  saveFile: (onSuccess?: () => void) => void;
}

const useMonacoEditor = ({ saveFile }: UseMonacoEditorProps) => {
  const monaco = useMonaco();
  const isConfigured = useRef<boolean>(false);

  useEffect(() => {
    if (monaco && !isConfigured.current) {
      isConfigured.current = true;
      configureMonaco(monaco);
    }
  }, [monaco]);

  useEffect(() => {
    let commandDisposer: (() => void) | undefined;

    if (monaco) {
      const commandId = "save-file";
      monaco.editor.registerCommand(commandId, () => {
        saveFile();
      });
      const keybindingRule = monaco.editor.addKeybindingRule({
        keybinding: monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
        command: commandId
      });

      commandDisposer = () => {
        if (keybindingRule) {
          keybindingRule.dispose();
        }
      };
    }

    return () => {
      if (commandDisposer) {
        commandDisposer();
      }
    };
  }, [monaco, saveFile]);

  return {
    monaco
  };
};

export default useMonacoEditor;
