import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

export const ExecuteSqlView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ database?: string }>(item.toolInput);
  const output = parseToolJson<{ database?: string; row_count?: number; ok?: boolean }>(
    item.toolOutput
  );

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Database</p>
            <p className='font-medium font-mono text-xs'>
              {output?.database ?? input?.database ?? "default"}
            </p>
          </div>
          {output?.row_count !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Rows</p>
              <p className='font-medium font-mono text-xs'>{output.row_count.toLocaleString()}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
