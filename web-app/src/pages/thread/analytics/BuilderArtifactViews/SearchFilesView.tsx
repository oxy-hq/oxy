import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

type SearchFileMatch = {
  path?: string;
  size_bytes?: number;
};

export const SearchFilesView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ pattern?: string }>(item.toolInput);
  const output = parseToolJson<{ files?: SearchFileMatch[]; count?: number }>(item.toolOutput);
  const files = output?.files ?? [];

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='col-span-2 rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Pattern</p>
            <p className='break-all font-medium font-mono text-xs'>{input?.pattern ?? "?"}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Matches</p>
            <p className='font-medium font-mono text-xs'>
              {(output?.count ?? files.length).toLocaleString()}
            </p>
          </div>
        </div>

        <div className='flex min-h-0 flex-1 flex-col'>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Files</p>
          {files.length > 0 ? (
            <div className='min-h-0 flex-1 space-y-2 overflow-auto rounded border bg-muted/20 p-3'>
              {files.map((file) => (
                <div
                  key={`${file.path ?? "file"}-${file.size_bytes ?? 0}`}
                  className='rounded border bg-background px-3 py-2'
                >
                  <p className='break-all font-medium font-mono text-xs'>{file.path ?? "?"}</p>
                  {file.size_bytes !== undefined && (
                    <p className='mt-1 text-[11px] text-muted-foreground'>
                      {file.size_bytes.toLocaleString()} bytes
                    </p>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className='rounded border bg-muted/20 px-3 py-2'>
              <p className='text-muted-foreground text-xs'>No matching files found.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
