import { useQuery } from "@tanstack/react-query";
import { Columns2, GitMerge, Loader2, Play, RotateCcw, Upload, WrapText, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { getLanguageFromFileName } from "@/components/FileEditor/constants";
import { BaseMonacoEditor } from "@/components/MonacoEditor";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/shadcn/sheet";
import useRevertFile from "@/hooks/api/files/useRevertFile";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { FileService, ProjectService } from "@/services/api";
import type { FileStatus } from "@/types/file";
import { MergeConflictEditor } from "./MergeConflictEditor";

const DEFAULT_MESSAGE = "Auto-commit: Oxy changes";
const MIN_PANEL_WIDTH = 420;
const SPLIT_VIEW_MIN_WIDTH = 720;

const STATUS_STYLES: Record<string, { label: string; className: string }> = {
  A: { label: "A", className: "text-emerald-400" },
  M: { label: "M", className: "text-amber-400" },
  D: { label: "D", className: "text-red-400" },
  R: { label: "R", className: "text-blue-400" },
  U: { label: "!", className: "text-rose-400" }
};

interface ChangesPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  diffSummary: FileStatus[];
  isPushing: boolean;
  pushLabel: string;
  onPush: (message: string) => void;
  isConflict?: boolean;
  onAbortConflict?: () => Promise<void>;
  onContinueRebase?: () => Promise<void>;
  onConflictResolved?: () => void;
}

interface FileDiffProps {
  file: FileStatus;
  splitView: boolean;
  onConflictResolved?: () => void;
}

function RegularFileDiff({ file, splitView }: FileDiffProps) {
  const { project, branchName } = useCurrentProjectBranch();
  const pathb64 = encodeBase64(file.path);
  const isAdded = file.status === "A";
  const isDeleted = file.status === "D";
  const language = getLanguageFromFileName(file.path);

  const { data: originalContent = "" } = useQuery({
    queryKey: ["file-from-git", project.id, branchName, file.path],
    queryFn: () => FileService.getFileFromGit(project.id, pathb64, branchName),
    enabled: !isAdded,
    retry: false
  });

  const { data: currentContent = "" } = useQuery({
    queryKey: ["file-current", project.id, branchName, file.path],
    queryFn: () => FileService.getFile(project.id, pathb64, branchName),
    enabled: !isDeleted,
    retry: false
  });

  return (
    <BaseMonacoEditor
      value={currentContent}
      original={isAdded ? "" : originalContent}
      diffMode
      splitView={splitView}
      language={language}
      path={file.path}
      height='100%'
      options={{
        readOnly: true,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontSize: 12,
        lineNumbers: "on",
        wordWrap: "on",
        wrappingStrategy: "advanced"
      }}
    />
  );
}

function FileDiff({ file, splitView, onConflictResolved }: FileDiffProps) {
  if (file.status === "U") {
    return <MergeConflictEditor file={file} onResolved={onConflictResolved ?? (() => {})} />;
  }
  return <RegularFileDiff file={file} splitView={splitView} />;
}

export const ChangesPanel = ({
  open,
  onOpenChange,
  diffSummary,
  isPushing,
  pushLabel,
  onPush,
  isConflict = false,
  onAbortConflict,
  onContinueRebase,
  onConflictResolved
}: ChangesPanelProps) => {
  const { project, branchName } = useCurrentProjectBranch();
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [message, setMessage] = useState(DEFAULT_MESSAGE);
  const [isAborting, setIsAborting] = useState(false);
  const [isContinuing, setIsContinuing] = useState(false);
  const [resolvingFile, setResolvingFile] = useState<{
    path: string;
    side: "mine" | "theirs";
  } | null>(null);
  const [unresolvingPath, setUnresolvingPath] = useState<string | null>(null);

  // Snapshot of paths that had status "U" when the panel opened in conflict mode.
  // Used to limit "Undo resolve" to files that were originally conflicted.
  const [originalConflictPaths, setOriginalConflictPaths] = useState<Set<string>>(new Set());

  const [splitView, setSplitView] = useState(true);
  const [panelWidth, setPanelWidth] = useState(() => Math.min(window.innerWidth - 80, 1200));
  const canSplitView = panelWidth >= SPLIT_VIEW_MIN_WIDTH;
  const effectiveSplitView = canSplitView && splitView;
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const revertFile = useRevertFile();
  const isDragging = useRef(false);
  const dragStartX = useRef(0);
  const dragStartWidth = useRef(0);

  const unresolvedCount = diffSummary.filter((f) => f.status === "U").length;
  const allResolved = isConflict && unresolvedCount === 0 && diffSummary.length > 0;

  // Snapshot which files were "U" the moment the panel opens in conflict mode.
  // This lets us show "Undo resolve" only for files that WERE conflicted, not
  // every non-U file in the list.
  useEffect(() => {
    if (open && isConflict) {
      setOriginalConflictPaths((prev) => {
        if (prev.size > 0) return prev; // already captured
        return new Set(diffSummary.filter((f) => f.status === "U").map((f) => f.path));
      });
    } else if (!open) {
      setOriginalConflictPaths(new Set());
    }
  }, [open, isConflict, diffSummary]);

  // Auto-select first file when panel opens or file list changes
  useEffect(() => {
    if (open && diffSummary.length > 0) {
      setSelectedPath((prev) => {
        if (prev && diffSummary.some((f) => f.path === prev)) return prev;
        return diffSummary[0].path;
      });
    }
  }, [open, diffSummary]);

  const handleResolveFile = async (filePath: string, side: "mine" | "theirs") => {
    if (!project?.id || !branchName) return;
    setResolvingFile({ path: filePath, side });
    try {
      const result = await ProjectService.resolveConflictFile(
        project.id,
        branchName,
        filePath,
        side
      );
      if (result.success) {
        onConflictResolved?.();
      } else {
        toast.error("Failed to resolve file", {
          action: result.message
            ? { label: "Show details", onClick: () => toast.message(result.message) }
            : undefined
        });
      }
    } catch {
      toast.error("Failed to resolve file");
    } finally {
      setResolvingFile(null);
    }
  };

  const handleUnresolveFile = async (filePath: string) => {
    if (!project?.id || !branchName) return;
    setUnresolvingPath(filePath);
    try {
      const result = await ProjectService.unresolveConflictFile(project.id, branchName, filePath);
      if (result.success) {
        onConflictResolved?.();
      } else {
        toast.error("Failed to undo resolution", {
          action: result.message
            ? { label: "Show details", onClick: () => toast.message(result.message) }
            : undefined
        });
      }
    } catch {
      toast.error("Failed to undo resolution");
    } finally {
      setUnresolvingPath(null);
    }
  };

  const handleContinue = async () => {
    if (!onContinueRebase) return;
    setIsContinuing(true);
    try {
      await onContinueRebase();
    } finally {
      setIsContinuing(false);
    }
  };

  const handleAbort = async () => {
    if (!onAbortConflict) return;
    setIsAborting(true);
    try {
      await onAbortConflict();
    } finally {
      setIsAborting(false);
    }
  };

  const handleSubmit = () => {
    if (!message.trim() || isPushing) return;
    onPush(message.trim());
    onOpenChange(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      isDragging.current = true;
      dragStartX.current = e.clientX;
      dragStartWidth.current = panelWidth;

      const onMouseMove = (ev: MouseEvent) => {
        if (!isDragging.current) return;
        const delta = dragStartX.current - ev.clientX;
        const next = Math.max(
          MIN_PANEL_WIDTH,
          Math.min(window.innerWidth - 80, dragStartWidth.current + delta)
        );
        setPanelWidth(next);
      };

      const onMouseUp = () => {
        isDragging.current = false;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [panelWidth]
  );

  const selectedFile = diffSummary.find((f) => f.path === selectedPath) ?? null;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side='right'
        data-testid='changes-panel'
        style={{ width: panelWidth, maxWidth: "calc(100vw - 80px)" }}
        className='flex flex-col gap-0 p-0'
      >
        {/* Drag handle on the left edge */}
        <div
          aria-hidden='true'
          onMouseDown={handleDragStart}
          className='absolute inset-y-0 left-0 z-10 w-1 cursor-col-resize transition-colors hover:bg-primary/30'
        />

        <SheetHeader className='border-border/40 border-b px-4 py-3 pr-12'>
          <SheetTitle className='flex items-center gap-2 font-mono text-sm'>
            {isConflict ? (
              <>
                <GitMerge className='h-3.5 w-3.5 text-amber-400' />
                <span>Merge conflicts</span>
                {unresolvedCount > 0 && (
                  <span className='rounded bg-amber-500/10 px-1.5 py-0.5 font-mono text-[11px] text-amber-400'>
                    {unresolvedCount} remaining
                  </span>
                )}
                {allResolved && (
                  <span className='rounded bg-emerald-500/10 px-1.5 py-0.5 font-mono text-[11px] text-emerald-400'>
                    all resolved
                  </span>
                )}
              </>
            ) : (
              <>
                Changes
                <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground'>
                  {diffSummary.length}
                </span>
              </>
            )}
            {canSplitView && !isConflict && (
              <div className='ml-auto flex items-center gap-0.5'>
                <button
                  type='button'
                  onClick={() => setSplitView(true)}
                  title='Split view'
                  className={`flex h-6 w-6 items-center justify-center rounded transition-colors ${splitView ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/40 hover:text-foreground"}`}
                >
                  <Columns2 className='h-3.5 w-3.5' />
                </button>
                <button
                  type='button'
                  onClick={() => setSplitView(false)}
                  title='Inline view'
                  className={`flex h-6 w-6 items-center justify-center rounded transition-colors ${!splitView ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/40 hover:text-foreground"}`}
                >
                  <WrapText className='h-3.5 w-3.5' />
                </button>
              </div>
            )}
          </SheetTitle>
        </SheetHeader>

        {/* Main area — resizable file list / diff split */}
        <ResizablePanelGroup direction='horizontal' className='min-h-0 flex-1'>
          <ResizablePanel
            defaultSize={22}
            minSize={15}
            maxSize={45}
            className='flex flex-col overflow-y-auto bg-sidebar-background'
          >
            {(() => {
              // In conflict mode: show unresolved (U) files first, then resolved ones with a divider
              const uFiles = isConflict ? diffSummary.filter((f) => f.status === "U") : [];
              const doneFiles = isConflict ? diffSummary.filter((f) => f.status !== "U") : [];
              const list = isConflict ? [...uFiles, ...doneFiles] : diffSummary;

              return list.map((file, idx) => {
                const style = STATUS_STYLES[file.status] ?? STATUS_STYLES.M;
                const name = file.path.split("/").pop() ?? file.path;
                const dir = file.path.includes("/")
                  ? file.path.slice(0, file.path.lastIndexOf("/"))
                  : "";
                const isSelected = file.path === selectedPath;
                const pathb64 = encodeBase64(file.path);
                const isReverting = revertFile.isPending && revertFile.variables === pathb64;
                const isResolving = resolvingFile?.path === file.path;
                const isUnresolving = unresolvingPath === file.path;
                // Show divider at the boundary between U and resolved files
                const showDivider =
                  isConflict && uFiles.length > 0 && doneFiles.length > 0 && idx === uFiles.length;

                return (
                  <div key={file.path}>
                    {showDivider && (
                      <div className='mx-2 my-1 flex items-center gap-2'>
                        <div className='h-px flex-1 bg-border/30' />
                        <span className='font-mono text-[9px] text-muted-foreground/30 uppercase tracking-widest'>
                          resolved
                        </span>
                        <div className='h-px flex-1 bg-border/30' />
                      </div>
                    )}
                    <div
                      className={`group flex w-full flex-col transition-colors hover:bg-accent/30 ${isSelected ? "bg-accent/50" : ""}`}
                    >
                      <div className='flex w-full items-center gap-1 pr-1'>
                        <button
                          type='button'
                          onClick={() => setSelectedPath(file.path)}
                          className='flex min-w-0 flex-1 items-start gap-2 px-3 py-2 text-left'
                        >
                          <span
                            className={`mt-0.5 shrink-0 font-bold font-mono text-[10px] uppercase ${style.className}`}
                          >
                            {style.label}
                          </span>
                          <div className='min-w-0 flex-1'>
                            <div className='truncate font-mono text-foreground text-xs'>{name}</div>
                            {dir && (
                              <div className='truncate font-mono text-[10px] text-muted-foreground/50'>
                                {dir}
                              </div>
                            )}
                          </div>
                        </button>
                        {!isConflict && (
                          <button
                            type='button'
                            onClick={() => revertFile.mutate(pathb64)}
                            disabled={isReverting}
                            title='Discard changes'
                            data-testid='changes-panel-discard-button'
                            className='invisible flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground/40 transition-colors hover:bg-destructive/15 hover:text-destructive disabled:opacity-40 group-hover:visible'
                          >
                            {isReverting ? (
                              <Loader2 className='h-3 w-3 animate-spin' />
                            ) : (
                              <RotateCcw className='h-3 w-3' />
                            )}
                          </button>
                        )}
                      </div>

                      {/* Undo resolve — only for files that were originally conflicted */}
                      {isConflict &&
                        file.status !== "U" &&
                        originalConflictPaths.has(file.path) && (
                          <div className='flex gap-1 pr-2 pb-1.5 pl-8'>
                            <button
                              type='button'
                              disabled={isUnresolving}
                              onClick={() => handleUnresolveFile(file.path)}
                              title='Restore conflict markers'
                              className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-amber-500/40 hover:bg-amber-500/8 hover:text-amber-400 disabled:opacity-40'
                            >
                              {isUnresolving ? (
                                <Loader2 className='h-2.5 w-2.5 animate-spin' />
                              ) : (
                                <RotateCcw className='h-2.5 w-2.5' />
                              )}
                              Undo resolve
                            </button>
                          </div>
                        )}

                      {/* Per-file quick resolution buttons */}
                      {isConflict && file.status === "U" && (
                        <div className='flex gap-1 pr-2 pb-1.5 pl-8'>
                          <button
                            type='button'
                            disabled={!!resolvingFile}
                            onClick={() => handleResolveFile(file.path, "mine")}
                            className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/8 hover:text-primary disabled:opacity-40'
                          >
                            {isResolving && resolvingFile?.side === "mine" ? (
                              <Loader2 className='h-2.5 w-2.5 animate-spin' />
                            ) : null}
                            Use Mine
                          </button>
                          <button
                            type='button'
                            disabled={!!resolvingFile}
                            onClick={() => handleResolveFile(file.path, "theirs")}
                            className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground disabled:opacity-40'
                          >
                            {isResolving && resolvingFile?.side === "theirs" ? (
                              <Loader2 className='h-2.5 w-2.5 animate-spin' />
                            ) : null}
                            Use Theirs
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                );
              });
            })()}
          </ResizablePanel>

          <ResizableHandle className='bg-border/40 hover:bg-border' />

          <ResizablePanel className='relative min-h-0'>
            {selectedFile ? (
              <FileDiff
                key={`${selectedFile.path}-${effectiveSplitView}`}
                file={selectedFile}
                splitView={effectiveSplitView}
                onConflictResolved={onConflictResolved}
              />
            ) : (
              <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
                Select a file to view changes
              </div>
            )}
          </ResizablePanel>
        </ResizablePanelGroup>

        {/* Footer */}
        {isConflict ? (
          <div className='flex items-center gap-2 border-border/40 border-t px-3 py-2'>
            <button
              type='button'
              onClick={handleContinue}
              disabled={!allResolved || isContinuing || !onContinueRebase}
              data-testid='changes-panel-continue-button'
              title={
                allResolved
                  ? undefined
                  : `Resolve ${unresolvedCount} remaining file${unresolvedCount === 1 ? "" : "s"} first`
              }
              className='flex items-center gap-1.5 rounded bg-gradient-to-b from-[#3550FF] to-[#2A40CC] px-3 py-1 font-medium text-white text-xs shadow-[#0B1033]/40 shadow-sm transition-all hover:from-[#5D73FF] hover:to-[#3550FF] disabled:opacity-40'
            >
              {isContinuing ? (
                <Loader2 className='h-3 w-3 animate-spin' />
              ) : (
                <Play className='h-3 w-3' />
              )}
              {isContinuing ? "Saving…" : "Save resolution"}
            </button>
            <button
              type='button'
              onClick={handleAbort}
              disabled={isAborting || !onAbortConflict}
              className='flex items-center gap-1.5 rounded border border-destructive/30 px-3 py-1 text-destructive text-xs transition-colors hover:bg-destructive/10 disabled:opacity-50'
            >
              {isAborting ? (
                <Loader2 className='h-3 w-3 animate-spin' />
              ) : (
                <X className='h-3 w-3' />
              )}
              {isAborting ? "Aborting…" : "Abort"}
            </button>
            {unresolvedCount > 0 && (
              <span className='ml-auto font-mono text-[10px] text-muted-foreground/50'>
                {unresolvedCount} file{unresolvedCount === 1 ? "" : "s"} remaining
              </span>
            )}
          </div>
        ) : (
          <div className='flex flex-col border-border/40 border-t'>
            <div className='border-border/40 border-b px-3 py-2'>
              <span className='font-mono text-[10px] text-muted-foreground/50 uppercase tracking-widest'>
                commit message
              </span>
            </div>
            <div className='px-3 py-2.5'>
              <textarea
                ref={textareaRef}
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                onKeyDown={handleKeyDown}
                rows={2}
                placeholder='Describe your changes…'
                data-testid='changes-panel-commit-message'
                className='w-full resize-none bg-transparent font-mono text-foreground text-sm placeholder:text-muted-foreground/30 focus:outline-none'
              />
            </div>
            <div className='flex items-center justify-between border-border/40 border-t px-3 py-2'>
              <span className='font-mono text-[10px] text-muted-foreground/35'>⌘↵ to push</span>
              <button
                type='button'
                onClick={handleSubmit}
                disabled={!message.trim() || isPushing}
                data-testid='changes-panel-push-button'
                className='flex items-center gap-1.5 rounded bg-gradient-to-b from-[#3550FF] to-[#2A40CC] px-3 py-1 font-medium text-white text-xs shadow-[#0B1033]/40 shadow-sm transition-all hover:from-[#5D73FF] hover:to-[#3550FF] disabled:opacity-50'
              >
                {isPushing ? (
                  <Loader2 className='h-3 w-3 animate-spin' />
                ) : (
                  <Upload className='h-3 w-3' />
                )}
                {pushLabel}
              </button>
            </div>
          </div>
        )}
      </SheetContent>
    </Sheet>
  );
};
