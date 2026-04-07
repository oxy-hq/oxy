import { CheckCircle2, X } from "lucide-react";
import { useMemo } from "react";

import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { cn } from "@/libs/shadcn/utils";
import { computeLineDiff } from "./graphUtils";

export const GenericFileDiff = ({ change }: { change: BuilderProposedChange }) => {
  const filename = change.filePath.split("/").pop() ?? change.filePath;
  const dir = change.filePath.includes("/")
    ? change.filePath.slice(0, change.filePath.lastIndexOf("/"))
    : "";
  const isNew = !change.oldContent;

  const { added, removed } = useMemo(() => {
    const ops = computeLineDiff(change.oldContent, change.newContent);
    return {
      added: ops.filter((op) => op.type === "add").length,
      removed: ops.filter((op) => op.type === "remove").length
    };
  }, [change.oldContent, change.newContent]);

  return (
    <div className='relative space-y-8'>
      <div className='space-y-1'>
        <div className='flex items-center gap-2'>
          <span className='font-mono text-[10px] text-muted-foreground/60 uppercase tracking-widest'>
            File Change
          </span>
          {change.status === "accepted" && (
            <span className='flex items-center gap-0.5 rounded bg-emerald-500/15 px-1.5 py-0.5 font-bold text-[10px] text-emerald-600 uppercase tracking-wide dark:text-emerald-400'>
              <CheckCircle2 className='h-2.5 w-2.5' /> Accepted
            </span>
          )}
          {change.status === "rejected" && (
            <span className='flex items-center gap-0.5 rounded bg-destructive/15 px-1.5 py-0.5 font-bold text-[10px] text-destructive uppercase tracking-wide'>
              <X className='h-2.5 w-2.5' /> Skipped
            </span>
          )}
          {change.status === "pending" && (added > 0 || removed > 0) && (
            <span className='rounded bg-emerald-500/15 px-1.5 py-0.5 font-bold text-[10px] text-emerald-600 dark:text-emerald-400'>
              {isNew ? "new file" : `+${added} −${removed}`}
            </span>
          )}
        </div>
        {change.description && (
          <p className='text-muted-foreground text-xs'>{change.description}</p>
        )}
      </div>

      <div
        className={cn(
          "rounded-lg border-2 px-4 py-3 shadow-sm",
          change.status === "accepted"
            ? "border-emerald-500/40 bg-emerald-500/5"
            : change.status === "rejected"
              ? "border-destructive/40 bg-destructive/5"
              : "border-primary/50 bg-primary/5"
        )}
      >
        <p
          className={cn(
            "font-mono text-[10px] uppercase tracking-wider",
            change.status === "accepted"
              ? "text-emerald-500"
              : change.status === "rejected"
                ? "text-destructive"
                : "text-primary/60"
          )}
        >
          {isNew ? "New File" : "Modified"}
        </p>
        <p className='font-bold text-foreground text-sm'>{filename}</p>
        {dir && <p className='font-mono text-[10px] text-muted-foreground'>{dir}/</p>}
        {(added > 0 || removed > 0) && (
          <p className='mt-0.5 text-[11px] text-muted-foreground'>
            {added > 0 && <span className='text-emerald-500'>+{added} lines</span>}
            {added > 0 && removed > 0 && " · "}
            {removed > 0 && <span className='text-destructive'>−{removed} lines</span>}
          </p>
        )}
      </div>
    </div>
  );
};
