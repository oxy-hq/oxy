import { JSX, useRef, useState } from "react";
import FileEditor, { FileEditorRef, FileState } from "@/components/FileEditor";
import EditorHeader from "../EditorHeader";
import { cn } from "@/libs/shadcn/utils";

export interface EditorPageWrapperProps {
  pathb64: string;
  onSaved?: () => void;
  preview?: JSX.Element;
  headerActions?: JSX.Element;
  className?: string;
  pageContentClassName?: string;
  editorClassName?: string;
  onFileValueChange?: (value: string) => void;
}

const EditorPageWrapper = ({
  pathb64,
  preview,
  headerActions,
  editorClassName,
  pageContentClassName,
  className,
  onSaved,
  onFileValueChange,
}: EditorPageWrapperProps) => {
  const filePath = atob(pathb64 ?? "");
  const [fileState, setFileState] = useState<FileState>("saved");
  const fileEditorRef = useRef<FileEditorRef>(null);

  const onSave = () => {
    if (fileEditorRef.current) {
      fileEditorRef.current.save();
    }
  };

  return (
    <div className={cn("flex h-full flex-col", className)}>
      <div className={cn("flex-1 flex overflow-hidden", pageContentClassName)}>
        <div
          className={cn(
            "flex-1 flex flex-col bg-editor-background",
            editorClassName,
          )}
        >
          <EditorHeader
            filePath={filePath}
            fileState={fileState}
            actions={headerActions}
            onSave={onSave}
          />
          <FileEditor
            ref={fileEditorRef}
            fileState={fileState}
            pathb64={pathb64 ?? ""}
            onFileStateChange={setFileState}
            onSaved={onSaved}
            onValueChange={onFileValueChange}
          />
        </div>
        {preview}
      </div>
    </div>
  );
};

export default EditorPageWrapper;
