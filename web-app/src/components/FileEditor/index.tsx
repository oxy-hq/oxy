import Editor, { OnMount, useMonaco } from "@monaco-editor/react";
import useFile from "@/hooks/api/useFile";
import useSaveFile from "@/hooks/api/useSaveFile";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Loader2 } from "lucide-react";
import { monacoGitHubDarkDefaultTheme } from "./github-dark-theme";

export type FileState = "saved" | "modified" | "saving";

export interface FileEditorRef {
  save: () => void;
}

interface Props {
  pathb64: string;
  fileState: FileState;
  onFileStateChange: (state: FileState) => void;
  onValueChange?: (value: string) => void;
  onSaved?: () => void;
}

const FileEditor = forwardRef<FileEditorRef, Props>(
  ({ pathb64, fileState, onFileStateChange, onValueChange, onSaved }, ref) => {
    const monaco = useMonaco();
    const editorRef = useRef<unknown>(null);

    useImperativeHandle(ref, () => ({
      save: () => {
        if (monaco && monaco.editor) {
          const editor = monaco.editor.getModels()[0];
          if (editor) {
            handleSaveFile(editor.getValue());
          }
        }
      },
    }));

    useEffect(() => {
      if (monaco) {
        monaco.editor.defineTheme("github-dark", monacoGitHubDarkDefaultTheme);
        monaco.editor.setTheme("github-dark");
      }
    }, [monaco]);

    const fileName = atob(pathb64);
    const { data: fileContent } = useFile(pathb64);
    const { mutate: saveFile } = useSaveFile();
    useEffect(() => {
      onValueChange?.(fileContent || "");
    }, [fileContent, onValueChange]);

    const handleSaveFile = (data: string) => {
      if (fileState === "saving") return;
      saveFile(
        { pathb64: pathb64 ?? "", data },
        {
          onSuccess: () => {
            onFileStateChange("saved");
            onSaved?.();
          },
          onError: () => onFileStateChange("modified"),
        },
      );
    };

    const getLanguageFromFileName = (fileName: string): string => {
      const extension = fileName.split(".").pop()?.toLowerCase() ?? "";
      const languageMap: Record<string, string> = {
        js: "javascript",
        jsx: "javascript",
        ts: "typescript",
        tsx: "typescript",
        py: "python",
        java: "java",
        cpp: "cpp",
        c: "c",
        cs: "csharp",
        go: "go",
        rs: "rust",
        rb: "ruby",
        php: "php",
        html: "html",
        css: "css",
        json: "json",
        md: "markdown",
        yaml: "yaml",
        yml: "yaml",
        sql: "sql",
        txt: "plaintext",
      };
      return languageMap[extension] ?? "plaintext";
    };

    const handleEditorMount: OnMount = (editor, monaco) => {
      editorRef.current = editor;
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        handleSaveFile(editor.getValue());
      });
    };

    return (
      <Editor
        theme="github-dark"
        height="100%"
        width="100%"
        language={getLanguageFromFileName(fileName)}
        value={fileContent}
        loading={
          <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
        }
        options={{
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          formatOnPaste: true,
          formatOnType: true,
          automaticLayout: true,
        }}
        onChange={(value) => {
          onValueChange?.(value || "");
          onFileStateChange("modified");
        }}
        onMount={handleEditorMount}
      />
    );
  },
);

export default FileEditor;
