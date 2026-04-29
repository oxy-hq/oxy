import { CircleCheck, Clock, FolderMinus, FolderOpen, FolderPlus, X } from "lucide-react";
import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

function OperationIcon({ operation }: { operation: string }) {
  if (operation === "create") return <FolderPlus className='h-4 w-4 text-muted-foreground' />;
  if (operation === "delete") return <FolderMinus className='h-4 w-4 text-muted-foreground' />;
  return <FolderOpen className='h-4 w-4 text-muted-foreground' />;
}

export const ManageDirectoryView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    operation?: string;
    path?: string;
    new_path?: string;
    description?: string;
  }>(item.toolInput);
  const output = parseToolJson<{ answer?: string; status?: string }>(item.toolOutput);

  const operation = input?.operation ?? "unknown";
  const path = input?.path ?? "?";
  const newPath = input?.new_path;
  const description = input?.description;

  const answer = output?.answer;
  const isPending = !answer && output?.status === "awaiting_response";
  const accepted = typeof answer === "string" && answer.toLowerCase().includes("accept");
  const rejected = typeof answer === "string" && answer.toLowerCase().includes("reject");

  return (
    <div className='flex h-full flex-col gap-4 overflow-auto p-4'>
      <div className='rounded border bg-muted/30 px-2.5 py-2'>
        <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Status</p>
        <div className='mt-0.5 flex items-center gap-1.5'>
          {isPending ? (
            <>
              <Clock className='h-3.5 w-3.5 text-amber-500' />
              <p className='font-medium text-xs'>Awaiting confirmation</p>
            </>
          ) : accepted ? (
            <>
              <CircleCheck className='h-3.5 w-3.5 text-emerald-500' />
              <p className='font-medium text-xs'>Accepted</p>
            </>
          ) : rejected ? (
            <>
              <X className='h-3.5 w-3.5 text-destructive' />
              <p className='font-medium text-xs'>Rejected</p>
            </>
          ) : (
            <>
              <CircleCheck className='h-3.5 w-3.5 text-emerald-500' />
              <p className='font-medium text-xs'>Completed</p>
            </>
          )}
        </div>
      </div>

      <div className='grid grid-cols-1 gap-2'>
        <div className='rounded border bg-muted/30 px-2.5 py-2'>
          <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Operation</p>
          <div className='mt-0.5 flex items-center gap-1.5'>
            <OperationIcon operation={operation} />
            <p className='font-medium font-mono text-xs capitalize'>{operation}</p>
          </div>
        </div>

        <div className='rounded border bg-muted/30 px-2.5 py-2'>
          <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Path</p>
          <p className='mt-0.5 break-all font-medium font-mono text-xs'>{path}</p>
        </div>

        {newPath && (
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>New Path</p>
            <p className='mt-0.5 break-all font-medium font-mono text-xs'>{newPath}</p>
          </div>
        )}
      </div>

      {description && (
        <div>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Description</p>
          <div className='rounded border bg-muted/20 px-3 py-2'>
            <p className='whitespace-pre-wrap text-xs'>{description}</p>
          </div>
        </div>
      )}
    </div>
  );
};
