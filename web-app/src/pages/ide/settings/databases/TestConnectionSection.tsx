import { AlertCircle, CheckCircle2Icon, ExternalLink } from "lucide-react";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { useTestDatabaseConnection } from "@/hooks/api/databases/useTestDatabaseConnection";

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
        {isTesting ? <Spinner /> : "Test Connection"}
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
        <div className='space-y-2 rounded border border-info/30 bg-info/10 p-3'>
          <div className='flex items-start gap-2'>
            <AlertCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-info' />
            <div className='flex-1 space-y-2'>
              <p className='font-medium text-info text-sm'>{testConnection.ssoMessage}</p>
              {testConnection.ssoTimeout && (
                <p className='text-info text-xs'>Timeout: {testConnection.ssoTimeout} seconds</p>
              )}
              <a
                href={testConnection.ssoUrl}
                target='_blank'
                rel='noopener noreferrer'
                className='inline-flex items-center gap-1 text-info text-sm hover:underline'
              >
                Open authentication page
                <ExternalLink className='h-3 w-3' />
              </a>
            </div>
          </div>
        </div>
      )}

      {/* Test Result */}
      {showTestResult &&
        testConnection.result &&
        (testConnection.result.success ? (
          <Alert>
            <CheckCircle2Icon />
            <AlertTitle>{testConnection.result.message}</AlertTitle>
            <AlertDescription>
              Connection time: {testConnection.result.connection_time_ms}ms
            </AlertDescription>
          </Alert>
        ) : (
          <ErrorAlert
            title={testConnection.result.message}
            message={
              <div>
                <p>Connection time: {testConnection.result.connection_time_ms}ms</p>
                {testConnection.result.error_details && (
                  <p>{testConnection.result.error_details}</p>
                )}
              </div>
            }
          />
        ))}

      {/* General Error */}
      {showTestResult && testConnection.error && !testConnection.result && (
        <ErrorAlert message={testConnection.error} />
      )}
    </div>
  );
}
