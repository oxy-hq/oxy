import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

export const ReadFileView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    path?: string;
    start_line?: number;
    end_line?: number;
  }>(item.toolInput);

  const output = parseToolJson<{
    content?: string;
    total_lines?: number;
    start_line?: number;
    end_line?: number;
    truncated?: boolean;
  }>(item.toolOutput);

  const filePath = input?.path ?? "?";
  const requestedRange =
    input?.start_line !== undefined || input?.end_line !== undefined
      ? `${input?.start_line ?? 1}-${input?.end_line ?? "end"}`
      : "start-end";
  const returnedRange =
    output?.start_line !== undefined && output?.end_line !== undefined
      ? `${output.start_line}-${output.end_line}`
      : null;
  const raw = output?.content ?? "";
  const content = raw
    .split("\n")
    .map((line) => line.replace(/^\d+ \| /, ""))
    .join("\n");

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='col-span-2 rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Path</p>
            <p className='break-all font-medium font-mono text-xs'>{filePath}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Requested</p>
            <p className='font-medium font-mono text-xs'>{requestedRange}</p>
          </div>
          {returnedRange && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Returned</p>
              <p className='font-medium font-mono text-xs'>{returnedRange}</p>
            </div>
          )}
          {output?.total_lines !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>
                Total Lines
              </p>
              <p className='font-medium font-mono text-xs'>{output.total_lines.toLocaleString()}</p>
            </div>
          )}
          {output?.truncated !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Truncated</p>
              <p className='font-medium font-mono text-xs'>{output.truncated ? "Yes" : "No"}</p>
            </div>
          )}
        </div>

        <div className='flex min-h-0 flex-1 flex-col'>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Content</p>
          {content ? (
            <div className='min-h-0 flex-1 overflow-auto rounded border bg-muted/50'>
              <pre className='whitespace-pre-wrap p-3 font-mono text-[11px]'>{content}</pre>
            </div>
          ) : (
            <p className='text-muted-foreground text-xs'>No file content returned.</p>
          )}
        </div>
      </div>
    </div>
  );
};
