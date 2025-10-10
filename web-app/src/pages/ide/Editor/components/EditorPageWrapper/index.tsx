import { JSX, useRef, useState, useEffect } from "react";
import FileEditor, { FileEditorRef, FileState } from "@/components/FileEditor";
import EditorHeader from "../EditorHeader";
import { cn } from "@/libs/shadcn/utils";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/shadcn/resizable";

const MIN_PANE_MIN_PERCENT = 10;

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
}

const EditorPageWrapper = ({
  pathb64,
  preview,
  headerActions,
  editorClassName,
  pageContentClassName,
  className,
  readOnly,
  git = false,
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

  const onShowDiff = () => {
    if (fileEditorRef.current) {
      fileEditorRef.current.toggleDiffView();
    }
  };

  // Determine whether we should use split pane (when preview exists and file types match)
  const isSql = filePath.endsWith(".sql");
  const shouldSplit =
    !!preview &&
    (isSql ||
      filePath.endsWith(".agent.yml") ||
      filePath.endsWith(".workflow.yml") ||
      filePath.endsWith(".app.yml"));
  // detect narrow viewport width (switch preview orientation for agent/app files)
  const [isNarrowViewport, setIsNarrowViewport] = useState(false);
  useEffect(() => {
    const onResize = () => {
      try {
        setIsNarrowViewport(window.innerWidth < 800);
      } catch {
        /* ignore */
      }
    };
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const isAgentOrApp =
    filePath.endsWith(".agent.yml") || filePath.endsWith(".app.yml");
  let orient: "horizontal" | "vertical";
  if (isSql) orient = "horizontal";
  else if (isAgentOrApp && isNarrowViewport) orient = "vertical";
  else orient = "horizontal";
  const storageKey = `split:${filePath}:${orient}`;
  let groupDirection: "horizontal" | "vertical";
  if (isSql) groupDirection = "vertical";
  else if (isAgentOrApp)
    groupDirection = isNarrowViewport ? "vertical" : "horizontal";
  else groupDirection = "horizontal";
  const containerRef = useRef<HTMLDivElement | null>(null);

  // initialize percent from storage safely and keep a ref in sync so
  // event handlers can access the latest value without re-registering.
  const initialPercent = (() => {
    try {
      const v = storageKey ? localStorage.getItem(storageKey) : null;
      return v ? Number(v) : 50;
    } catch {
      return 50;
    }
  })();

  const [percent, setPercent] = useState<number>(initialPercent);

  useEffect(() => {
    if (!shouldSplit) return;
    try {
      const v = storageKey ? localStorage.getItem(storageKey) : null;
      if (v) setPercent(Number(v));
      else setPercent(50);
    } catch {
      /* ignore */
    }
  }, [storageKey, shouldSplit]);

  // Small helpers to reduce duplication in the split rendering
  function EditorPane({ isSql }: { isSql: boolean }) {
    return (
      <div
        className={cn(
          "flex flex-col bg-editor-background overflow-hidden min-h-0",
          !isSql && "min-w-0",
          editorClassName,
        )}
        style={{ width: "100%", height: "100%" }}
      >
        <EditorHeader
          filePath={filePath}
          fileState={fileState}
          actions={headerActions}
          onSave={onSave}
          isReadonly={readOnly}
          onShowDiff={onShowDiff}
          git={git}
        />
        <FileEditor
          ref={fileEditorRef}
          fileState={fileState}
          pathb64={pathb64 ?? ""}
          onFileStateChange={setFileState}
          onSaved={onSaved}
          onValueChange={onFileValueChange}
          readOnly={readOnly}
          git={git}
        />
      </div>
    );
  }

  function PreviewPane({ isSql }: { isSql: boolean }) {
    return (
      <div
        className={cn(
          "flex-1 flex flex-col overflow-hidden min-h-0",
          !isSql && "min-w-0",
        )}
        style={{ width: "100%", height: "100%" }}
      >
        {preview}
      </div>
    );
  }

  return (
    <div className={cn("flex h-full flex-col", className)}>
      <div
        ref={containerRef}
        className={cn("flex-1 flex overflow-hidden", pageContentClassName)}
      >
        {shouldSplit ? (
          // Use project's Resizable wrapper which uses react-resizable-panels
          <ResizablePanelGroup
            direction={groupDirection}
            className="h-full flex-1 flex overflow-hidden"
          >
            <ResizablePanel
              minSize={MIN_PANE_MIN_PERCENT}
              defaultSize={percent}
            >
              <EditorPane isSql={isSql} />
            </ResizablePanel>

            <ResizableHandle withHandle />

            <ResizablePanel>
              <PreviewPane isSql={isSql} />
            </ResizablePanel>
          </ResizablePanelGroup>
        ) : (
          <>
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
                isReadonly={readOnly}
                onShowDiff={onShowDiff}
                git={git}
              />
              <FileEditor
                ref={fileEditorRef}
                fileState={fileState}
                pathb64={pathb64 ?? ""}
                onFileStateChange={setFileState}
                onSaved={onSaved}
                onValueChange={onFileValueChange}
                readOnly={readOnly}
                git={git}
              />
            </div>
            {preview}
          </>
        )}
      </div>
    </div>
  );
};

export default EditorPageWrapper;
