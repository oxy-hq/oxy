import { Columns2, GitMerge, WrapText } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/shadcn/sheet";
import { useAuth } from "@/contexts/AuthContext";
import useDiffSummary from "@/hooks/api/files/useDiffSummary";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { FileStatus } from "@/types/file";
import { CommitFooter } from "./CommitFooter";
import { ConflictFooter } from "./ConflictFooter";
import { FileDiff } from "./FileDiff";
import { FileList } from "./FileList";
import { useConflictActions } from "./useConflictActions";

const MIN_PANEL_WIDTH = 420;
const SPLIT_VIEW_MIN_WIDTH = 720;

interface ChangesPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isPushing: boolean;
  pushLabel: string;
  onPush: (message: string) => void;
  isConflict?: boolean;
  onAbortConflict?: () => Promise<void>;
  onContinueRebase?: () => Promise<void>;
  onConflictResolved?: () => void;
}

export const ChangesPanel = ({
  open,
  onOpenChange,
  isPushing,
  pushLabel,
  onPush,
  isConflict = false,
  onAbortConflict,
  onContinueRebase,
  onConflictResolved
}: ChangesPanelProps) => {
  const { isLocalMode } = useAuth();
  const { project, branchName } = useCurrentProjectBranch();
  // Only fetch the diff while the panel is open — the header derives
  // "has uncommitted changes" from the lighter revision-info query.
  const { data: diffSummaryData, isLoading: isDiffLoading } = useDiffSummary(open);
  const diffSummary: FileStatus[] = diffSummaryData ?? [];

  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  // Snapshot of paths originally "U" so "Undo resolve" only offers the
  // files that started conflicted.
  const [originalConflictPaths, setOriginalConflictPaths] = useState<Set<string>>(new Set());
  const [splitView, setSplitView] = useState(true);
  const [panelWidth, setPanelWidth] = useState(() => Math.min(window.innerWidth - 80, 1200));

  const canSplitView = panelWidth >= SPLIT_VIEW_MIN_WIDTH;
  const effectiveSplitView = canSplitView && splitView;

  const isDragging = useRef(false);
  const dragStartX = useRef(0);
  const dragStartWidth = useRef(0);

  const unresolvedCount = diffSummary.filter((f) => f.status === "U").length;
  const allResolved = isConflict && unresolvedCount === 0 && diffSummary.length > 0;

  const {
    resolvingFile,
    unresolvingPath,
    isAborting,
    isContinuing,
    handleResolveFile,
    handleUnresolveFile,
    handleContinue,
    handleAbort
  } = useConflictActions({
    workspaceId: project?.id,
    branchName,
    onResolved: onConflictResolved,
    onAbortConflict,
    onContinueRebase
  });

  // Both branches must return `prev` when the state shouldn't change —
  // `diffSummary` is re-created each render while the query is loading,
  // and unconditional `setState` triggers an infinite loop.
  useEffect(() => {
    if (open && isConflict) {
      setOriginalConflictPaths((prev) => {
        if (prev.size > 0) return prev;
        const uPaths = diffSummary.filter((f) => f.status === "U").map((f) => f.path);
        if (uPaths.length === 0) return prev;
        return new Set(uPaths);
      });
    } else if (!open) {
      setOriginalConflictPaths((prev) => (prev.size === 0 ? prev : new Set()));
    }
  }, [open, isConflict, diffSummary]);

  useEffect(() => {
    if (open && diffSummary.length > 0) {
      setSelectedPath((prev) => {
        if (prev && diffSummary.some((f) => f.path === prev)) return prev;
        return diffSummary[0].path;
      });
    }
  }, [open, diffSummary]);

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

  if (isLocalMode) return null;

  const selectedFile = diffSummary.find((f) => f.path === selectedPath) ?? null;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side='right'
        data-testid='changes-panel'
        style={{ width: panelWidth, maxWidth: "calc(100vw - 80px)" }}
        className='flex flex-col gap-0 p-0'
      >
        <div
          aria-hidden='true'
          onMouseDown={handleDragStart}
          className='absolute inset-y-0 left-0 z-10 w-1 cursor-col-resize transition-colors hover:bg-primary/30'
        />

        <SheetHeader className='border-border/40 border-b px-4 py-3 pr-12'>
          <SheetTitle className='flex items-center gap-2 font-mono text-sm'>
            {isConflict ? (
              <>
                <GitMerge className='h-3.5 w-3.5 text-warning' />
                <span>Merge conflicts</span>
                {unresolvedCount > 0 && (
                  <span className='rounded bg-warning/10 px-1.5 py-0.5 font-mono text-[11px] text-warning'>
                    {unresolvedCount} remaining
                  </span>
                )}
                {allResolved && (
                  <span className='rounded bg-success/10 px-1.5 py-0.5 font-mono text-[11px] text-success'>
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

        <ResizablePanelGroup direction='horizontal' className='min-h-0 flex-1'>
          <ResizablePanel
            defaultSize={22}
            minSize={15}
            maxSize={45}
            className='flex flex-col overflow-y-auto bg-sidebar-background'
          >
            <FileList
              diffSummary={diffSummary}
              isLoading={isDiffLoading}
              selectedPath={selectedPath}
              onSelect={setSelectedPath}
              isConflict={isConflict}
              originalConflictPaths={originalConflictPaths}
              resolvingFile={resolvingFile}
              unresolvingPath={unresolvingPath}
              onResolveFile={(p, side) => void handleResolveFile(p, side)}
              onUnresolveFile={(p) => void handleUnresolveFile(p)}
            />
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

        {isConflict ? (
          <ConflictFooter
            unresolvedCount={unresolvedCount}
            allResolved={allResolved}
            isAborting={isAborting}
            isContinuing={isContinuing}
            onContinue={() => void handleContinue()}
            onAbort={() => void handleAbort()}
            canContinue={!!onContinueRebase}
            canAbort={!!onAbortConflict}
          />
        ) : (
          <CommitFooter
            isPushing={isPushing}
            pushLabel={pushLabel}
            onPush={onPush}
            onClose={() => onOpenChange(false)}
          />
        )}
      </SheetContent>
    </Sheet>
  );
};
