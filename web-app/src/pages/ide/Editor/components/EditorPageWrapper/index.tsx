import { JSX, useState, useEffect, useMemo } from "react";
import FileEditor from "@/components/FileEditor";
import EditorHeader from "../EditorHeader";
import { cn } from "@/libs/shadcn/utils";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/shadcn/resizable";
import { FileEditorProvider } from "@/components/FileEditor/FileEditorContext";

const MIN_PANE_SIZE_PERCENT = 10;
const NARROW_VIEWPORT_BREAKPOINT = 800;

export interface EditorPageWrapperRef {
  setContent: (newContent: string) => void;
}

export interface EditorPageWrapperProps {
  pathb64: string;
  onSaved?: () => void;
  onChanged?: (content: string) => void;
  preview?: JSX.Element;
  headerActions?: JSX.Element;
  className?: string;
  pageContentClassName?: string;
  editorClassName?: string;
  readOnly?: boolean;
  git?: boolean;
  defaultDirection?: "horizontal" | "vertical";
  customEditor?: JSX.Element;
}

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
  onChanged,
  defaultDirection = "horizontal",
  customEditor,
}: EditorPageWrapperProps) => {
  const filePath = atob(pathb64 ?? "");

  const isNarrowViewport = useViewportDetection();
  const hasPreview = !!preview;

  const layoutDirection = useMemo(() => {
    return isNarrowViewport ? "vertical" : defaultDirection;
  }, [defaultDirection, isNarrowViewport]);

  const storageKey = `ide:split:${filePath}`;

  const renderEditor = () => (
    <div
      className={cn(
        "flex flex-col bg-editor-background overflow-hidden min-h-0",
      )}
      style={{ width: "100%", height: "100%" }}
    >
      <EditorHeader
        readOnly={readOnly}
        actions={headerActions}
        filePath={filePath}
      />
      {customEditor ? (
        customEditor
      ) : (
        <FileEditor
          readOnly={readOnly}
          className={customEditor ? "hidden" : ""}
        />
      )}
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
    <FileEditorProvider
      pathb64={pathb64}
      git={git}
      onSaved={onSaved}
      onChanged={onChanged}
    >
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
    </FileEditorProvider>
  );
};

export default EditorPageWrapper;
