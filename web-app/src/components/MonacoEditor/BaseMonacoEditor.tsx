import Editor, { DiffEditor } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { Loader2 } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

export interface BaseMonacoEditorOptions {
  minimap?: { enabled: boolean };
  scrollBeyondLastLine?: boolean;
  formatOnPaste?: boolean;
  formatOnType?: boolean;
  automaticLayout?: boolean;
  readOnly?: boolean;
  fontSize?: number;
  lineNumbers?: "on" | "off" | "relative" | "interval";
  wordWrap?: "on" | "off" | "wordWrapColumn" | "bounded";
  tabSize?: number;
  renderSideBySide?: boolean;
}

export interface BaseMonacoEditorProps {
  value: string;
  onChange?: (value: string) => void;
  onMount?: (editor: editor.IStandaloneCodeEditor) => void;
  language?: string;
  theme?: "github-dark" | "vs-dark" | "light";
  height?: string;
  width?: string;
  className?: string;
  path?: string;
  options?: BaseMonacoEditorOptions;
  isLoading?: boolean;
  // Diff mode props
  diffMode?: boolean;
  original?: string;
}

const defaultOptions: BaseMonacoEditorOptions = {
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  formatOnPaste: true,
  formatOnType: true,
  automaticLayout: true,
  readOnly: false,
  fontSize: 13,
  lineNumbers: "on",
  tabSize: 2,
  wordWrap: "on",
};

const LoadingSpinner = () => (
  <div className="flex items-center justify-center h-full">
    <Loader2 className="w-4 h-4 animate-spin" />
  </div>
);

export default function BaseMonacoEditor({
  value,
  onChange,
  onMount,
  language = "plaintext",
  theme = "github-dark",
  height = "100%",
  width = "100%",
  className,
  path,
  options = {},
  isLoading = false,
  diffMode = false,
  original,
}: BaseMonacoEditorProps) {
  const mergedOptions = { ...defaultOptions, ...options };

  if (isLoading) {
    return <LoadingSpinner />;
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
          {diffMode && original !== undefined ? (
            <DiffEditor
              theme={theme}
              height={height}
              width={width}
              original={original}
              modified={value}
              language={language}
              loading={<LoadingSpinner />}
              options={{
                ...mergedOptions,
                readOnly: true,
                renderSideBySide: options.renderSideBySide ?? true,
              }}
            />
          ) : (
            <Editor
              path={path}
              theme={theme}
              height={height}
              width={width}
              defaultValue={value}
              language={language}
              value={value}
              loading={<LoadingSpinner />}
              options={mergedOptions}
              onChange={(v) => onChange?.(v || "")}
              onMount={onMount}
            />
          )}
        </div>
      </div>
    </>
  );
}
