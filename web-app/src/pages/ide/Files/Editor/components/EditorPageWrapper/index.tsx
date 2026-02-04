import { type JSX, useEffect, useMemo, useState } from "react";
import FileEditor from "@/components/FileEditor";
import { FileEditorProvider } from "@/components/FileEditor/FileEditorContext";
import { useNavigationBlock } from "@/components/FileEditor/hooks/useNavigationBlock";
import UnsavedChangesDialog from "@/components/FileEditor/UnsavedChangesDialog";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { cn } from "@/libs/shadcn/utils";
import EditorHeader from "../EditorHeader";

const MIN_PANE_SIZE_PERCENT = 10;
const NARROW_VIEWPORT_BREAKPOINT = 800;

export interface EditorPageWrapperRef {
  setContent: (newContent: string) => void;
}

export interface EditorPageWrapperProps {
  pathb64: string;
  onSaved?: (content?: string) => void;
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
  previewOnly?: boolean;
}

const useViewportDetection = (breakpoint: number = NARROW_VIEWPORT_BREAKPOINT) => {
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
  previewOnly = false
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
      className={cn("flex min-h-0 flex-col overflow-hidden bg-editor-background")}
      style={{ width: "100%", height: "100%" }}
    >
      <EditorHeader readOnly={readOnly} actions={headerActions} filePath={filePath} />
      {customEditor ? (
        customEditor
      ) : (
        <FileEditor readOnly={readOnly} className={customEditor ? "hidden" : ""} />
      )}
    </div>
  );

  const renderPreview = () =>
    preview ? (
      <div
        className='flex min-h-0 flex-1 flex-col overflow-hidden'
        style={{ width: "100%", height: "100%" }}
      >
        {preview}
      </div>
    ) : null;

  return (
    <FileEditorProvider pathb64={pathb64} git={git} onSaved={onSaved} onChanged={onChanged}>
      <EditorPageWrapperContent
        className={className}
        hasPreview={hasPreview}
        previewOnly={previewOnly}
        storageKey={storageKey}
        layoutDirection={layoutDirection}
        renderEditor={renderEditor}
        renderPreview={renderPreview}
      />
    </FileEditorProvider>
  );
};

interface EditorPageWrapperContentProps {
  className?: string;
  hasPreview: boolean;
  previewOnly?: boolean;
  storageKey: string;
  layoutDirection: "horizontal" | "vertical";
  renderEditor: () => JSX.Element;
  renderPreview: () => JSX.Element | null;
}

const EditorPageWrapperContent = ({
  className,
  hasPreview,
  previewOnly = false,
  storageKey,
  layoutDirection,
  renderEditor,
  renderPreview
}: EditorPageWrapperContentProps) => {
  const {
    state: { fileState },
    actions
  } = useFileEditorContext();

  const { unsavedChangesDialogOpen, setUnsavedChangesDialogOpen, blocker } =
    useNavigationBlock(fileState);

  const handleSaveAndNavigate = () => {
    actions.save(() => blocker.proceed?.());
  };

  const renderContent = () => {
    if (previewOnly) {
      return renderPreview();
    }
    if (hasPreview) {
      return (
        <ResizablePanelGroup
          autoSaveId={storageKey}
          direction={layoutDirection}
          className='flex h-full flex-1 overflow-hidden'
        >
          <ResizablePanel minSize={MIN_PANE_SIZE_PERCENT} defaultSize={50}>
            {renderEditor()}
          </ResizablePanel>

          <ResizableHandle withHandle />

          <ResizablePanel>{renderPreview()}</ResizablePanel>
        </ResizablePanelGroup>
      );
    }
    return renderEditor();
  };

  return (
    <>
      <div className={cn("flex h-full flex-col", className)}>
        <div className={cn("flex flex-1 overflow-hidden")}>{renderContent()}</div>
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
};

export default EditorPageWrapper;
