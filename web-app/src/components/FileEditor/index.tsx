import { useMemo } from "react";
import { BaseMonacoEditor } from "@/components/MonacoEditor";
import { getLanguageFromFileName } from "./constants";
import useMonacoEditor from "./hooks/useMonacoEditor";
import { useFileEditorContext } from "./useFileEditorContext";

export type FileState = "saved" | "modified" | "saving";

export interface FileEditorRef {
  save: () => void;
  toggleDiffView: () => void;
  setContent: (newContent: string) => void;
}

interface Props {
  readOnly?: boolean;
  className?: string;
}

const FileEditor = ({ readOnly = false, className }: Props) => {
  const {
    state: { fileName, content, originalContent, showDiff, isLoading },
    actions
  } = useFileEditorContext();

  useMonacoEditor({
    saveFile: actions.save
  });

  const language = useMemo(() => getLanguageFromFileName(fileName), [fileName]);

  if (content === null || content === undefined) {
    return null;
  }

  return (
    <BaseMonacoEditor
      path={`file://${fileName}`}
      value={content}
      onChange={actions.setContent}
      language={language}
      className={className}
      isLoading={isLoading}
      diffMode={showDiff}
      original={originalContent ?? undefined}
      height={originalContent ? "calc(100% - 50px)" : "100%"}
      options={{
        minimap: { enabled: true },
        scrollBeyondLastLine: true,
        readOnly: showDiff ? true : readOnly
      }}
    />
  );
};

export default FileEditor;
