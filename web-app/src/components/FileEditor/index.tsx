import Editor, { DiffEditor } from "@monaco-editor/react";
import useFile from "@/hooks/api/files/useFile";
import useFileGit from "@/hooks/api/files/useFileGit";
import { forwardRef, useEffect, useImperativeHandle, useState } from "react";
import { Loader2 } from "lucide-react";
import UnsavedChangesDialog from "./UnsavedChangesDialog";
import { useNavigationBlock } from "./hooks/useNavigationBlock";
import { getLanguageFromFileName } from "./constants";
import useMonacoEditor from "./hooks/useMonacoEditor";

export type FileState = "saved" | "modified" | "saving";

export interface FileEditorRef {
  save: () => void;
  toggleDiffView: () => void;
}

interface Props {
  pathb64: string;
  fileState: FileState;
  onFileStateChange: (state: FileState) => void;
  onValueChange?: (value: string) => void;
  onSaved?: () => void;
  readOnly?: boolean;
}

const FileEditor = forwardRef<FileEditorRef, Props>(
  (
    {
      pathb64,
      fileState,
      onFileStateChange,
      onValueChange,
      onSaved,
      readOnly = false,
    },
    ref,
  ) => {
    const fileName = atob(pathb64);
    const { data: fileContent, isPending } = useFile(pathb64);
    const { data: originalContent } = useFileGit(pathb64);
    const [showDiff, setShowDiff] = useState(false);

    const [currentFileContent, setCurrentFileContent] = useState(fileContent);

    useEffect(() => {
      onValueChange?.(fileContent || "");
    }, [fileContent, onValueChange]);

    useEffect(() => {
      setCurrentFileContent(fileContent);
    }, [fileContent]);

    const { handleEditorMount, handleSaveFile } = useMonacoEditor({
      onFileStateChange,
      onSaved,
      fileState,
      pathb64,
    });

    const { unsavedChangesDialogOpen, setUnsavedChangesDialogOpen, blocker } =
      useNavigationBlock(fileState);

    useImperativeHandle(ref, () => ({
      save: () => handleSaveFile(),
      toggleDiffView: () => setShowDiff(!showDiff),
    }));

    const handleSaveAndNavigate = () => {
      handleSaveFile(() => blocker.proceed?.());
    };

    if (isPending) {
      return (
        <div className="flex items-center justify-center h-full">
          <Loader2 className="animate-spin h-4 w-4" />
        </div>
      );
    }

    if (!fileContent && fileContent != "") {
      return null;
    }

    return (
      <>
        {/*
          Wrap Monaco in a relative, overflow-hidden container so that during
          rapid resizes the Monaco canvas/scrollbar cannot visually spill
          outside of the editor bounds and overlap sibling panes.
        */}
        <div className="relative h-full w-full overflow-hidden">
          <div className="absolute inset-0">
            {showDiff && originalContent ? (
              <DiffEditor
                theme="github-dark"
                height={originalContent ? "calc(100% - 50px)" : "100%"}
                width="100%"
                original={originalContent}
                modified={currentFileContent ?? ""}
                language={getLanguageFromFileName(fileName)}
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
                defaultValue={fileContent ?? ""}
                language={getLanguageFromFileName(fileName)}
                value={currentFileContent}
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
                onChange={(value) => {
                  if (readOnly) return;
                  onValueChange?.(value || "");
                  onFileStateChange("modified");
                  setCurrentFileContent(value);
                }}
                onMount={handleEditorMount}
              />
            )}
          </div>
        </div>

        <UnsavedChangesDialog
          open={unsavedChangesDialogOpen}
          onOpenChange={setUnsavedChangesDialogOpen}
          onDiscard={() => {
            setUnsavedChangesDialogOpen(false);
            blocker.proceed?.();
          }}
          onSave={handleSaveAndNavigate}
        />
      </>
    );
  },
);

export default FileEditor;
