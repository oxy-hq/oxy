import { AlertCircle, CheckCircle, ExternalLink, Loader2, XCircle } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { useTestDatabaseConnection } from "@/hooks/api/databases/useTestDatabaseConnection";
import { cn } from "@/libs/utils/cn";

interface TestConnectionSectionProps {
  isTesting: boolean;
  showTestResult: boolean;
  testConnection: ReturnType<typeof useTestDatabaseConnection>;
  onTest: () => void;
  disabled?: boolean;
}

export function TestConnectionSection({
  isTesting,
  showTestResult,
  testConnection,
  onTest,
  disabled = false
}: TestConnectionSectionProps) {
  return (
    <div className='space-y-2 border-t pt-2'>
      <Button
        type='button'
        variant={"default"}
        size='sm'
        onClick={onTest}
        disabled={isTesting || disabled}
        className='w-full'
      >
        {isTesting ? (
          <>
            <Loader2 className='h-4 w-4 animate-spin' />
            Testing Connection...
          </>
        ) : (
          "Test Connection"
        )}
      </Button>

      {/* Show test progress */}
      {showTestResult && testConnection.progress.length > 0 && (
        <div className='space-y-1 rounded bg-muted p-2 text-xs'>
          {testConnection.progress.slice(-3).map((msg, idx) => (
            <div key={idx} className='text-muted-foreground'>
              • {msg}
            </div>
          ))}
        </div>
      )}

      {/* Browser Auth Required */}
      {showTestResult && testConnection.ssoUrl && (
        <div className='space-y-2 rounded border border-blue-200 bg-blue-50 p-3 dark:border-blue-800 dark:bg-blue-950'>
          <div className='flex items-start gap-2'>
            <AlertCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-blue-600' />
            <div className='flex-1 space-y-2'>
              <p className='font-medium text-blue-900 text-sm dark:text-blue-100'>
                {testConnection.ssoMessage}
              </p>
              {testConnection.ssoTimeout && (
                <p className='text-blue-700 text-xs dark:text-blue-300'>
                  Timeout: {testConnection.ssoTimeout} seconds
                </p>
              )}
              <a
                href={testConnection.ssoUrl}
                target='_blank'
                rel='noopener noreferrer'
                className='inline-flex items-center gap-1 text-blue-600 text-sm hover:underline dark:text-blue-400'
              >
                Open authentication page
                <ExternalLink className='h-3 w-3' />
              </a>
            </div>
          </div>
        </div>
      )}

      {/* Test Result */}
      {showTestResult && testConnection.result && (
        <div
          className={cn(
            "space-y-1 rounded p-3",
            testConnection.result.success
              ? "border border-green-200 bg-green-50 dark:border-green-800 dark:bg-green-950"
              : "border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-950"
          )}
        >
          <div className='flex items-start gap-2'>
            {testConnection.result.success ? (
              <CheckCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-green-600' />
            ) : (
              <XCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-red-600' />
            )}
            <div className='flex-1'>
              <p
                className={cn(
                  "font-medium text-sm",
                  testConnection.result.success
                    ? "text-green-900 dark:text-green-100"
                    : "text-red-900 dark:text-red-100"
                )}
              >
                {testConnection.result.message}
              </p>
              {testConnection.result.connection_time_ms && (
                <p
                  className={cn(
                    "mt-1 text-xs",
                    testConnection.result.success
                      ? "text-green-700 dark:text-green-300"
                      : "text-red-700 dark:text-red-300"
                  )}
                >
                  Connection time: {testConnection.result.connection_time_ms}ms
                </p>
              )}
              {testConnection.result.error_details && (
                <pre className='mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded bg-red-100 p-2 text-xs dark:bg-red-900'>
                  {testConnection.result.error_details}
                </pre>
              )}
            </div>
          </div>
        </div>
      )}

      {/* General Error */}
      {showTestResult && testConnection.error && !testConnection.result && (
        <div className='rounded border border-red-200 bg-red-50 p-3 dark:border-red-800 dark:bg-red-950'>
          <div className='flex items-start gap-2'>
            <XCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-red-600' />
            <p className='text-red-900 text-sm dark:text-red-100'>{testConnection.error}</p>
          </div>
        </div>
      )}
    </div>
  );
}
