import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

export const SemanticQueryView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    topic?: string;
    dimensions?: string[];
    measures?: string[];
    limit?: number;
  }>(item.toolInput);
  const output = parseToolJson<{ database?: string; row_count?: number; ok?: boolean }>(
    item.toolOutput
  );

  const chips = [
    ...(input?.dimensions ?? []).map((value) => ({ label: value, kind: "dimension" })),
    ...(input?.measures ?? []).map((value) => ({ label: value, kind: "measure" }))
  ];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Topic</p>
            <p className='font-medium font-mono text-xs'>{input?.topic ?? "?"}</p>
          </div>
          {output?.database && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Database</p>
              <p className='font-medium font-mono text-xs'>{output.database}</p>
            </div>
          )}
          {input?.limit !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Limit</p>
              <p className='font-medium font-mono text-xs'>{input.limit.toLocaleString()}</p>
            </div>
          )}
          {output?.row_count !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Rows</p>
              <p className='font-medium font-mono text-xs'>{output.row_count.toLocaleString()}</p>
            </div>
          )}
        </div>

        {chips.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Fields</p>
            <div className='flex flex-wrap gap-1.5'>
              {chips.map((chip) => (
                <span
                  key={`${chip.kind}-${chip.label}`}
                  className='rounded-full bg-muted px-2.5 py-0.5 text-xs'
                >
                  {chip.label}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
