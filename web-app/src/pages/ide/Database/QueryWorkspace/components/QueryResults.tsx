import { Info } from "lucide-react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useDatabaseClient, { type SqlExecutionError } from "@/stores/useDatabaseClient";

/**
 * Render a `SqlExecutionError` as a structured block: SQLSTATE badge,
 * message, then the optional `detail`, `hint`, and SQL excerpt with a caret
 * pointing at `position` (1-based, char offset). Fields that aren't present
 * are silently skipped.
 */
function StructuredSqlError({ error }: { error: SqlExecutionError }) {
  return (
    <div className='space-y-2 text-left text-sm'>
      <div className='flex flex-wrap items-baseline gap-x-2'>
        {error.code && (
          <span className='rounded bg-error/20 px-1.5 py-0.5 font-mono text-error text-xs'>
            {error.code}
          </span>
        )}
        <span className='whitespace-pre-wrap font-medium'>{error.message}</span>
      </div>
      {error.detail && (
        <div className='whitespace-pre-wrap text-muted-foreground'>
          <span className='font-medium'>Detail:</span> {error.detail}
        </div>
      )}
      {error.hint && (
        <div className='whitespace-pre-wrap text-muted-foreground'>
          <span className='font-medium'>Hint:</span> {error.hint}
        </div>
      )}
      {error.sql && error.position != null && (
        <SqlPositionExcerpt sql={error.sql} position={error.position} />
      )}
    </div>
  );
}

/**
 * Show a one-line excerpt of the failing SQL with a caret under the
 * 1-based character offset reported by the server.
 */
function SqlPositionExcerpt({ sql, position }: { sql: string; position: number }) {
  // Server uses 1-based positions; convert to 0-based for slicing and clamp.
  const idx = Math.max(0, Math.min(sql.length, position - 1));
  // Pull just the line containing the offending position so a multi-line
  // statement doesn't render the whole query.
  const lineStart = sql.lastIndexOf("\n", idx - 1) + 1;
  const lineEndRaw = sql.indexOf("\n", idx);
  const lineEnd = lineEndRaw === -1 ? sql.length : lineEndRaw;
  const line = sql.slice(lineStart, lineEnd);
  const col = idx - lineStart;
  return (
    <pre className='mt-1 overflow-x-auto rounded bg-muted/40 p-2 font-mono text-xs leading-relaxed'>
      <div>{line}</div>
      <div>{`${" ".repeat(col)}^ position ${position}`}</div>
    </pre>
  );
}

export default function QueryResults() {
  const { tabs, activeTabId, setTabError } = useDatabaseClient();
  const activeTab = tabs.find((t) => t.id === activeTabId);

  if (!activeTab) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        <p className='text-sm'>Select a query to see results</p>
      </div>
    );
  }

  if (activeTab.isExecuting) {
    return (
      <div className='flex h-full flex-col items-center justify-center text-muted-foreground'>
        <Spinner className='mb-2 size-8' />
      </div>
    );
  }

  if (activeTab.error || activeTab.errorDetails) {
    const onDismiss = () => setTabError(activeTab.id, undefined, undefined);
    return (
      <div className='flex h-full flex-col items-center justify-center p-4'>
        {activeTab.errorDetails ? (
          <ErrorAlert title='Query failed' className='max-w-2xl' onDismiss={onDismiss}>
            <StructuredSqlError error={activeTab.errorDetails} />
          </ErrorAlert>
        ) : (
          <ErrorAlert message={activeTab.error} className='max-w-lg' onDismiss={onDismiss} />
        )}
      </div>
    );
  }

  if (!activeTab.results) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        <p className='text-sm'>No results to display</p>
      </div>
    );
  }

  const { result, resultFile, truncated } = activeTab.results;

  return (
    <div
      className='flex min-h-0 flex-1 flex-col overflow-hidden'
      style={{ width: "100%", height: "100%" }}
    >
      <div className='flex flex-1 flex-col overflow-hidden'>
        {truncated && (
          <div className='flex shrink-0 items-center gap-2 border-b bg-muted/40 px-3 py-1.5 text-muted-foreground text-xs'>
            <Info className='h-3.5 w-3.5 shrink-0' />
            <span>
              Showing the first 10,000 rows. Add a{" "}
              <code className='rounded bg-muted px-1 font-mono'>LIMIT</code> or filters to see the
              rest.
            </span>
          </div>
        )}
        <div className='flex-1 overflow-hidden'>
          <SqlResultsTable result={result} resultFile={resultFile} />
        </div>
      </div>
    </div>
  );
}
