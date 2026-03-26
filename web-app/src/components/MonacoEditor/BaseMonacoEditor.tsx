import Editor, { DiffEditor, type Monaco } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";
import type { editor } from "monaco-editor";
import { useEffect, useRef } from "react";
import { configureMonaco } from "@/components/FileEditor/monacoConfig";
import { cn } from "@/libs/shadcn/utils";

export interface BaseMonacoEditorOptions {
  minimap?: { enabled: boolean };
  scrollBeyondLastLine?: boolean;
  formatOnPaste?: boolean;
  formatOnType?: boolean;
  automaticLayout?: boolean;
  readOnly?: boolean;
  readOnlyMessage?: { value: string };
  fontSize?: number;
  lineNumbers?: "on" | "off" | "relative" | "interval";
  wordWrap?: "on" | "off" | "wordWrapColumn" | "bounded";
  wrappingStrategy?: "simple" | "advanced";
  tabSize?: number;
  renderSideBySide?: boolean;
}

export interface BaseMonacoEditorProps {
  value: string;
  onChange?: (value: string) => void;
  onMount?: (editor: editor.IStandaloneCodeEditor, monaco: Monaco) => void;
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
  splitView?: boolean;
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
  wordWrap: "on"
};

const LoadingSpinner = () => (
  <div className='flex h-full items-center justify-center'>
    <Loader2 className='h-4 w-4 animate-spin' />
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
  splitView = true
}: BaseMonacoEditorProps) {
  const mergedOptions = { ...defaultOptions, ...options };
  const diffEditorRef = useRef<editor.IStandaloneDiffEditor | null>(null);

  // Imperatively toggle renderSideBySide — bypasses @monaco-editor/react's
  // options-tracking which can miss updates in React 19 concurrent rendering.
  useEffect(() => {
    diffEditorRef.current?.updateOptions({ renderSideBySide: splitView });
  }, [splitView]);

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
        <div className='absolute inset-0'>
          {diffMode && original !== undefined ? (
            <DiffEditor
              theme={theme}
              height={height}
              width={width}
              original={original}
              modified={value}
              language={language}
              loading={<LoadingSpinner />}
              beforeMount={configureMonaco}
              onMount={(e) => {
                diffEditorRef.current = e;
                // Set renderSideBySide immediately on mount — the options prop
                // alone is unreliable because Monaco initialises async and may
                // ignore the value set during createDiffEditor.
                e.updateOptions({ renderSideBySide: splitView });
                // Re-apply after the Sheet open animation (~500ms) completes.
                // Monaco auto-switches to inline mode when the container is
                // narrow during the animation; this overrides that once the
                // container reaches its final width.
                setTimeout(() => {
                  diffEditorRef.current?.updateOptions({ renderSideBySide: splitView });
                }, 600);
              }}
              options={{
                ...mergedOptions,
                readOnly: true,
                renderSideBySide: splitView
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
              beforeMount={configureMonaco}
              onChange={(v) => onChange?.(v || "")}
              onMount={(ed, monaco) => onMount?.(ed, monaco)}
            />
          )}
        </div>
      </div>
    </>
  );
}
