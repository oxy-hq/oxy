import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

type ValidationErrorItem = {
  file?: string;
  error?: string;
};

export const ValidateProjectView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ file_path?: string }>(item.toolInput);
  const output = parseToolJson<{
    valid?: boolean;
    file?: string;
    valid_count?: number;
    error_count?: number;
    errors?: Array<string | ValidationErrorItem>;
  }>(item.toolOutput);

  const requestedFile = input?.file_path;
  const scope = requestedFile ? "Single file" : "Whole project";
  const isValid = output?.valid ?? false;
  const validatedFile = output?.file ?? requestedFile;
  const validCount = output?.valid_count;
  const errorCount = output?.error_count ?? output?.errors?.length ?? 0;
  const errors = (output?.errors ?? []).map((entry) =>
    typeof entry === "string" ? { error: entry } : entry
  );

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Scope</p>
            <p className='font-medium font-mono text-xs'>{scope}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Status</p>
            <p className='font-medium font-mono text-xs'>{isValid ? "Valid" : "Errors found"}</p>
          </div>
          {validatedFile && (
            <div className='col-span-2 rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>File</p>
              <p className='break-all font-medium font-mono text-xs'>{validatedFile}</p>
            </div>
          )}
          {validCount !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>
                Valid Files
              </p>
              <p className='font-medium font-mono text-xs'>{validCount.toLocaleString()}</p>
            </div>
          )}
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Error Count</p>
            <p className='font-medium font-mono text-xs'>{errorCount.toLocaleString()}</p>
          </div>
        </div>

        <div className='flex min-h-0 flex-1 flex-col'>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Results</p>
          {errors.length > 0 ? (
            <div className='min-h-0 flex-1 space-y-2 overflow-auto rounded border bg-muted/20 p-3'>
              {errors.map((entry, index) => (
                <div
                  key={`${entry.file ?? "validation"}-${entry.error ?? index}`}
                  className='rounded border bg-background px-3 py-2'
                >
                  {entry.file && (
                    <p className='mb-1 break-all font-medium font-mono text-xs'>{entry.file}</p>
                  )}
                  <p className='whitespace-pre-wrap text-[11px] text-muted-foreground'>
                    {entry.error ?? "Unknown validation error"}
                  </p>
                </div>
              ))}
            </div>
          ) : (
            <div className='rounded border bg-muted/20 px-3 py-2'>
              <p className='text-muted-foreground text-xs'>
                {isValid ? "Validation passed with no errors." : "No validation output available."}
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
