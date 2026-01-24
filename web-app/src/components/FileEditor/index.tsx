import Editor, { DiffEditor } from "@monaco-editor/react";
import { useMemo } from "react";
import { Loader2 } from "lucide-react";
import { getLanguageFromFileName } from "./constants";
import useMonacoEditor from "./hooks/useMonacoEditor";
import { cn } from "@/libs/shadcn/utils";
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
    actions,
  } = useFileEditorContext();

  useMonacoEditor({
    saveFile: actions.save,
  });

  const language = useMemo(() => getLanguageFromFileName(fileName), [fileName]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="animate-spin h-4 w-4" />
      </div>
    );
  }

  if (content === null || content === undefined) {
    return null;
  }

  return (
    <>
      {/*
          Wrap Monaco in a relative, overflow-hidden container so that during
          rapid resizes the Monaco canvas/scrollbar cannot visually spill
          outside of the editor bounds and overlap sibling panes.
        */}
      <div
        className={cn("relative h-full w-full overflow-hidden", className)}
        onKeyDown={(e) => {
          // Stop keyboard events from bubbling to parent ResizablePanelGroup
          // which captures Space and other keys for panel resizing
          e.stopPropagation();
        }}
      >
        <div className="absolute inset-0">
          {showDiff && originalContent ? (
            <DiffEditor
              theme="github-dark"
              height={originalContent ? "calc(100% - 50px)" : "100%"}
              width="100%"
              original={originalContent}
              modified={content}
              language={language}
              loading={
                <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
              }
              options={{
                minimap: { enabled: true },
                scrollBeyondLastLine: true,
                formatOnPaste: true,
                formatOnType: true,
                automaticLayout: true,
                readOnly: true,
                renderSideBySide: true,
              }}
            />
          ) : (
            <Editor
              path={"file://" + fileName}
              theme="github-dark"
              height={originalContent ? "calc(100% - 50px)" : "100%"}
              width="100%"
              defaultValue={content ?? ""}
              language={language}
              value={content}
              loading={
                <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
              }
              options={{
                minimap: { enabled: true },
                scrollBeyondLastLine: true,
                formatOnPaste: true,
                formatOnType: true,
                automaticLayout: true,
                readOnly: readOnly,
              }}
              onChange={(value) => actions.setContent(value || "")}
            />
          )}
        </div>
      </div>
    </>
  );
};

export default FileEditor;
