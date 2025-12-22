import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/utils/cn";
import {
  Loader2,
  CheckCircle,
  XCircle,
  AlertCircle,
  ExternalLink,
} from "lucide-react";
import { useTestDatabaseConnection } from "@/hooks/api/databases/useTestDatabaseConnection";

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
  disabled = false,
}: TestConnectionSectionProps) {
  return (
    <div className="space-y-2 pt-2 border-t">
      <Button
        type="button"
        variant={"default"}
        size="sm"
        onClick={onTest}
        disabled={isTesting || disabled}
        className="w-full"
      >
        {isTesting ? (
          <>
            <Loader2 className="h-4 w-4 animate-spin" />
            Testing Connection...
          </>
        ) : (
          "Test Connection"
        )}
      </Button>

      {/* Show test progress */}
      {showTestResult && testConnection.progress.length > 0 && (
        <div className="text-xs space-y-1 bg-muted p-2 rounded">
          {testConnection.progress.slice(-3).map((msg, idx) => (
            <div key={idx} className="text-muted-foreground">
              • {msg}
            </div>
          ))}
        </div>
      )}

      {/* Browser Auth Required */}
      {showTestResult && testConnection.ssoUrl && (
        <div className="bg-blue-50 dark:bg-blue-950 border border-blue-200 dark:border-blue-800 rounded p-3 space-y-2">
          <div className="flex items-start gap-2">
            <AlertCircle className="h-4 w-4 text-blue-600 mt-0.5 flex-shrink-0" />
            <div className="flex-1 space-y-2">
              <p className="text-sm font-medium text-blue-900 dark:text-blue-100">
                {testConnection.ssoMessage}
              </p>
              {testConnection.ssoTimeout && (
                <p className="text-xs text-blue-700 dark:text-blue-300">
                  Timeout: {testConnection.ssoTimeout} seconds
                </p>
              )}
              <a
                href={testConnection.ssoUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-sm text-blue-600 dark:text-blue-400 hover:underline"
              >
                Open authentication page
                <ExternalLink className="h-3 w-3" />
              </a>
            </div>
          </div>
        </div>
      )}

      {/* Test Result */}
      {showTestResult && testConnection.result && (
        <div
          className={cn(
            "rounded p-3 space-y-1",
            testConnection.result.success
              ? "bg-green-50 dark:bg-green-950 border border-green-200 dark:border-green-800"
              : "bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800",
          )}
        >
          <div className="flex items-start gap-2">
            {testConnection.result.success ? (
              <CheckCircle className="h-4 w-4 text-green-600 mt-0.5 flex-shrink-0" />
            ) : (
              <XCircle className="h-4 w-4 text-red-600 mt-0.5 flex-shrink-0" />
            )}
            <div className="flex-1">
              <p
                className={cn(
                  "text-sm font-medium",
                  testConnection.result.success
                    ? "text-green-900 dark:text-green-100"
                    : "text-red-900 dark:text-red-100",
                )}
              >
                {testConnection.result.message}
              </p>
              {testConnection.result.connection_time_ms && (
                <p
                  className={cn(
                    "text-xs mt-1",
                    testConnection.result.success
                      ? "text-green-700 dark:text-green-300"
                      : "text-red-700 dark:text-red-300",
                  )}
                >
                  Connection time: {testConnection.result.connection_time_ms}ms
                </p>
              )}
              {testConnection.result.error_details && (
                <pre className="text-xs mt-2 bg-red-100 dark:bg-red-900 p-2 rounded overflow-auto max-h-32 whitespace-pre-wrap">
                  {testConnection.result.error_details}
                </pre>
              )}
            </div>
          </div>
        </div>
      )}

      {/* General Error */}
      {showTestResult && testConnection.error && !testConnection.result && (
        <div className="bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded p-3">
          <div className="flex items-start gap-2">
            <XCircle className="h-4 w-4 text-red-600 mt-0.5 flex-shrink-0" />
            <p className="text-sm text-red-900 dark:text-red-100">
              {testConnection.error}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
