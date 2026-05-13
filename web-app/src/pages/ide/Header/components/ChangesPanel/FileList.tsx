import { RotateCcw } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useRevertFile from "@/hooks/api/files/useRevertFile";
import { encodeBase64 } from "@/libs/encoding";
import type { FileStatus } from "@/types/file";

const STATUS_STYLES: Record<string, { label: string; className: string }> = {
  A: { label: "A", className: "text-success" },
  M: { label: "M", className: "text-warning" },
  D: { label: "D", className: "text-destructive" },
  R: { label: "R", className: "text-info" },
  U: { label: "!", className: "text-destructive" }
};

interface Props {
  diffSummary: FileStatus[];
  isLoading: boolean;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  isConflict: boolean;
  originalConflictPaths: Set<string>;
  resolvingFile: { path: string; side: "mine" | "theirs" } | null;
  unresolvingPath: string | null;
  onResolveFile: (path: string, side: "mine" | "theirs") => void;
  onUnresolveFile: (path: string) => void;
}

export function FileList({
  diffSummary,
  isLoading,
  selectedPath,
  onSelect,
  isConflict,
  originalConflictPaths,
  resolvingFile,
  unresolvingPath,
  onResolveFile,
  onUnresolveFile
}: Props) {
  const revertFile = useRevertFile();

  if (isLoading && diffSummary.length === 0) {
    return (
      <div className='flex flex-1 items-center justify-center py-6 text-muted-foreground/60'>
        <Spinner className='size-3' />
      </div>
    );
  }

  const uFiles = isConflict ? diffSummary.filter((f) => f.status === "U") : [];
  const doneFiles = isConflict ? diffSummary.filter((f) => f.status !== "U") : [];
  const list = isConflict ? [...uFiles, ...doneFiles] : diffSummary;

  return (
    <>
      {list.map((file, idx) => {
        const style = STATUS_STYLES[file.status] ?? STATUS_STYLES.M;
        const name = file.path.split("/").pop() ?? file.path;
        const dir = file.path.includes("/") ? file.path.slice(0, file.path.lastIndexOf("/")) : "";
        const isSelected = file.path === selectedPath;
        const pathb64 = encodeBase64(file.path);
        const isReverting = revertFile.isPending && revertFile.variables === pathb64;
        const isResolving = resolvingFile?.path === file.path;
        const isUnresolving = unresolvingPath === file.path;
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
                  onClick={() => onSelect(file.path)}
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
                      <Spinner className='size-3' />
                    ) : (
                      <RotateCcw className='h-3 w-3' />
                    )}
                  </button>
                )}
              </div>

              {isConflict && file.status !== "U" && originalConflictPaths.has(file.path) && (
                <div className='flex gap-1 pr-2 pb-1.5 pl-8'>
                  <button
                    type='button'
                    disabled={isUnresolving}
                    onClick={() => onUnresolveFile(file.path)}
                    title='Restore conflict markers'
                    className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-warning/40 hover:bg-warning/8 hover:text-warning disabled:opacity-40'
                  >
                    {isUnresolving ? (
                      <Spinner className='size-2.5' />
                    ) : (
                      <RotateCcw className='h-2.5 w-2.5' />
                    )}
                    Undo resolve
                  </button>
                </div>
              )}

              {isConflict && file.status === "U" && (
                <div className='flex gap-1 pr-2 pb-1.5 pl-8'>
                  <button
                    type='button'
                    disabled={!!resolvingFile}
                    onClick={() => onResolveFile(file.path, "mine")}
                    className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/8 hover:text-primary disabled:opacity-40'
                  >
                    {isResolving && resolvingFile?.side === "mine" ? (
                      <Spinner className='size-2.5' />
                    ) : null}
                    Use Mine
                  </button>
                  <button
                    type='button'
                    disabled={!!resolvingFile}
                    onClick={() => onResolveFile(file.path, "theirs")}
                    className='flex h-5 items-center gap-1 rounded border border-border/50 px-2 font-mono text-[10px] text-muted-foreground transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground disabled:opacity-40'
                  >
                    {isResolving && resolvingFile?.side === "theirs" ? (
                      <Spinner className='size-2.5' />
                    ) : null}
                    Use Theirs
                  </button>
                </div>
              )}
            </div>
          </div>
        );
      })}
    </>
  );
}
