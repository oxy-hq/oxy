import { JSX, useRef, useState, useEffect, useMemo, useCallback } from "react";
import FileEditor, { FileEditorRef, FileState } from "@/components/FileEditor";
import EditorHeader from "../EditorHeader";
import { cn } from "@/libs/shadcn/utils";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/shadcn/resizable";

const MIN_PANE_SIZE_PERCENT = 10;
const NARROW_VIEWPORT_BREAKPOINT = 800;

export interface EditorPageWrapperProps {
  pathb64: string;
  onSaved?: () => void;
  preview?: JSX.Element;
  headerActions?: JSX.Element;
  className?: string;
  pageContentClassName?: string;
  editorClassName?: string;
  readOnly?: boolean;
  onFileValueChange?: (value: string) => void;
  git?: boolean;
  defaultDirection?: "horizontal" | "vertical";
}

// Custom hook for viewport detection
const useViewportDetection = (
  breakpoint: number = NARROW_VIEWPORT_BREAKPOINT,
) => {
  const [isNarrowViewport, setIsNarrowViewport] = useState(false);

  useEffect(() => {
    const handleResize = () => {
      try {
        setIsNarrowViewport(window.innerWidth < breakpoint);
      } catch {
        // Ignore errors (e.g., SSR)
      }
    };

    handleResize();
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [breakpoint]);

  return isNarrowViewport;
};

const EditorPageWrapper = ({
  pathb64,
  preview,
  headerActions,
  className,
  readOnly,
  git = false,
  onSaved,
  onFileValueChange,
  defaultDirection = "horizontal",
}: EditorPageWrapperProps) => {
  const filePath = atob(pathb64 ?? "");
  const [fileState, setFileState] = useState<FileState>("saved");
  const fileEditorRef = useRef<FileEditorRef>(null);

  const isNarrowViewport = useViewportDetection();
  const hasPreview = !!preview;

  const layoutDirection = useMemo(() => {
    return isNarrowViewport ? "vertical" : defaultDirection;
  }, [defaultDirection, isNarrowViewport]);

  const handleSave = useCallback(() => {
    fileEditorRef.current?.save();
  }, []);

  const handleShowDiff = useCallback(() => {
    fileEditorRef.current?.toggleDiffView();
  }, []);

  const storageKey = `ide:split:${filePath}`;

  const fileEditorProps = {
    ref: fileEditorRef,
    fileState,
    pathb64: pathb64 ?? "",
    onFileStateChange: setFileState,
    onSaved,
    onValueChange: onFileValueChange,
    readOnly,
    git,
  };

  const editorHeaderProps = {
    filePath,
    fileState,
    actions: headerActions,
    onSave: handleSave,
    isReadonly: readOnly,
    onShowDiff: handleShowDiff,
    git,
  };

  const renderEditor = () => (
    <div
      className={cn(
        "flex flex-col bg-editor-background overflow-hidden min-h-0",
      )}
      style={{ width: "100%", height: "100%" }}
    >
      <EditorHeader {...editorHeaderProps} />
      <FileEditor {...fileEditorProps} />
    </div>
  );

  const renderPreview = () =>
    preview ? (
      <div
        className="flex-1 flex flex-col overflow-hidden min-h-0"
        style={{ width: "100%", height: "100%" }}
      >
        {preview}
      </div>
    ) : null;

  return (
    <div className={cn("flex h-full flex-col", className)}>
      <div className={cn("flex-1 flex overflow-hidden")}>
        {hasPreview ? (
          <ResizablePanelGroup
            autoSaveId={storageKey}
            direction={layoutDirection}
            className="h-full flex-1 flex overflow-hidden"
          >
            <ResizablePanel minSize={MIN_PANE_SIZE_PERCENT} defaultSize={50}>
              {renderEditor()}
            </ResizablePanel>

            <ResizableHandle withHandle />

            <ResizablePanel>{renderPreview()}</ResizablePanel>
          </ResizablePanelGroup>
        ) : (
          renderEditor()
        )}
      </div>
    </div>
  );
};

export default EditorPageWrapper;
