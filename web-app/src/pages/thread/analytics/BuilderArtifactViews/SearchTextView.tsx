import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

interface MatchLine {
  file: string;
  line: string;
  text: string;
}

function parseContentLines(result: string): MatchLine[] {
  return result
    .split("\n")
    .filter(Boolean)
    .map((raw) => {
      const firstColon = raw.indexOf(":");
      const secondColon = firstColon >= 0 ? raw.indexOf(":", firstColon + 1) : -1;
      if (firstColon >= 0 && secondColon >= 0) {
        return {
          file: raw.slice(0, firstColon),
          line: raw.slice(firstColon + 1, secondColon),
          text: raw.slice(secondColon + 1)
        };
      }
      return { file: raw, line: "", text: "" };
    });
}

export const SearchTextView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ pattern?: string; glob?: string; output_mode?: string }>(
    item.toolInput
  );
  const output = parseToolJson<{ result?: string; count?: number; truncated?: boolean }>(
    item.toolOutput
  );

  const pattern = input?.pattern ?? "?";
  const glob = input?.glob ?? null;
  const outputMode = input?.output_mode ?? "content";
  const count = output?.count ?? 0;
  const truncated = output?.truncated ?? false;
  const result = output?.result ?? "";

  const contentLines = outputMode === "content" ? parseContentLines(result) : [];
  const fileLines = outputMode === "files_with_matches" ? result.split("\n").filter(Boolean) : [];

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='col-span-2 rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Pattern</p>
            <p className='break-all font-medium font-mono text-xs'>{pattern}</p>
          </div>
          {glob && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Glob</p>
              <p className='break-all font-medium font-mono text-xs'>{glob}</p>
            </div>
          )}
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Mode</p>
            <p className='font-medium font-mono text-xs'>{outputMode}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Matches</p>
            <p className='font-medium font-mono text-xs'>
              {count.toLocaleString()}
              {truncated && (
                <span className='ml-1 text-[10px] text-muted-foreground'>(truncated)</span>
              )}
            </p>
          </div>
        </div>

        {outputMode === "content" && (
          <div className='flex min-h-0 flex-1 flex-col'>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Matches</p>
            {contentLines.length > 0 ? (
              <div className='min-h-0 flex-1 space-y-1.5 overflow-auto rounded border bg-muted/20 p-3'>
                {contentLines.map((m, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: stable ordered list
                  <div key={i} className='rounded border bg-background px-3 py-2'>
                    <p className='mb-0.5 font-mono text-[10px] text-muted-foreground'>
                      {m.file}
                      {m.line ? `:${m.line}` : ""}
                    </p>
                    <p className='break-all font-mono text-xs'>{m.text}</p>
                  </div>
                ))}
              </div>
            ) : (
              <div className='rounded border bg-muted/20 px-3 py-2'>
                <p className='text-muted-foreground text-xs'>No matches found.</p>
              </div>
            )}
          </div>
        )}

        {outputMode === "files_with_matches" && (
          <div className='flex min-h-0 flex-1 flex-col'>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Files</p>
            {fileLines.length > 0 ? (
              <div className='min-h-0 flex-1 space-y-2 overflow-auto rounded border bg-muted/20 p-3'>
                {fileLines.map((file) => (
                  <div key={file} className='rounded border bg-background px-3 py-2'>
                    <p className='break-all font-medium font-mono text-xs'>{file}</p>
                  </div>
                ))}
              </div>
            ) : (
              <div className='rounded border bg-muted/20 px-3 py-2'>
                <p className='text-muted-foreground text-xs'>No matching files found.</p>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
