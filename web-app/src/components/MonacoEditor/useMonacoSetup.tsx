import { useMonaco } from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import {
  configureMonaco,
  configureMonacoEnvironment,
} from "@/components/FileEditor/monacoConfig";

configureMonacoEnvironment();

interface UseMonacoSetupProps {
  onSave?: () => void;
}

/**
 * Hook to configure Monaco editor with shared settings (theme, YAML schema, etc.)
 * and optionally register a Cmd/Ctrl+S save keybinding.
 */
export default function useMonacoSetup({ onSave }: UseMonacoSetupProps = {}) {
  const monaco = useMonaco();
  const isConfigured = useRef<boolean>(false);

  useEffect(() => {
    if (monaco && !isConfigured.current) {
      isConfigured.current = true;
      configureMonaco(monaco);
    }
  }, [monaco]);

  useEffect(() => {
    if (!monaco || !onSave) return;

    const commandId = "save-file";
    monaco.editor.registerCommand(commandId, () => {
      onSave();
    });
    const keybindingRule = monaco.editor.addKeybindingRule({
      keybinding: monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
      command: commandId,
    });

    return () => {
      keybindingRule?.dispose();
    };
  }, [monaco, onSave]);

  return { monaco };
}
