import { useMonaco } from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import {
  configureMonaco,
  configureMonacoEnvironment,
} from "@/components/FileEditor/monacoConfig";

configureMonacoEnvironment();

interface UseMonacoSetupProps {
  onSave?: () => void;
  onExecute?: () => void;
}

/**
 * Hook to configure Monaco editor with shared settings (theme, YAML schema, etc.)
 * and optionally register a Cmd/Ctrl+S save keybinding and Cmd/Ctrl+Enter execute keybinding.
 */
export default function useMonacoSetup({
  onSave,
  onExecute,
}: UseMonacoSetupProps = {}) {
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
    const command = monaco.editor.registerCommand(commandId, () => {
      onSave();
    });
    const keybindingRule = monaco.editor.addKeybindingRule({
      keybinding: monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
      command: commandId,
    });

    return () => {
      command.dispose();
      keybindingRule?.dispose();
    };
  }, [monaco, onSave]);

  useEffect(() => {
    if (!monaco || !onExecute) return;

    const commandId = "execute-file";
    const command = monaco.editor.registerCommand(commandId, () => {
      onExecute();
    });
    const keybindingRule = monaco.editor.addKeybindingRule({
      keybinding: monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      command: commandId,
    });

    return () => {
      command.dispose();
      keybindingRule?.dispose();
    };
  }, [monaco, onExecute]);

  return { monaco };
}
