import Editor from "@monaco-editor/react";
import useFile from "@/hooks/api/useFile";
import useSaveFile from "@/hooks/api/useSaveFile";
import { useEffect } from "react";
import { Loader2 } from "lucide-react";

export type FileState = "saved" | "modified" | "saving";

interface Props {
  pathb64: string;
  fileState: FileState;
  onFileStateChange: (state: FileState) => void;
  onValueChange?: (value: string) => void;
  onSaved?: () => void;
}

const FileEditor = ({
  pathb64,
  fileState,
  onFileStateChange,
  onValueChange,
  onSaved,
}: Props) => {
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

  return (
    <Editor
      theme="vs-dark"
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
      onMount={(editor, monaco) => {
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
          handleSaveFile(editor.getValue());
        });
      }}
    />
  );
};

export default FileEditor;
