import * as Sentry from "@sentry/react";
import { AlertTriangle } from "lucide-react";
import { useEffect } from "react";
import { isRouteErrorResponse, useRouteError } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";

/**
 * Router-level `errorElement`.
 *
 * React Router catches any render-phase throw within a route subtree and renders
 * the nearest `errorElement`. With none defined it falls back to its bare-bones
 * default screen ("Unexpected Application Error!… provide your own ErrorBoundary
 * or errorElement"). Crucially, the app-level Sentry `<ErrorBoundary>` wrapping
 * `<RouterProvider>` never sees these — React Router intercepts route render
 * errors first — so a single unguarded throw (e.g. a bad timestamp reaching
 * date-fns) took the whole page down to that raw screen.
 *
 * Mounting this at the router root guarantees an unexpected crash instead shows
 * a branded, recoverable page, and is still reported to Sentry.
 */
export function RouteErrorBoundary() {
  const error = useRouteError();

  useEffect(() => {
    // A thrown Response (e.g. a 404 from a loader) is expected navigation, not a
    // crash — don't page Sentry for it.
    if (!isRouteErrorResponse(error)) {
      Sentry.captureException(error);
    }
  }, [error]);

  const detail = isRouteErrorResponse(error)
    ? `${error.status} ${error.statusText}`
    : error instanceof Error
      ? error.message
      : undefined;

  return (
    <div className='flex min-h-screen w-full flex-col items-center justify-center gap-4 bg-background p-6 text-center'>
      <AlertTriangle className='h-10 w-10 text-destructive' />
      <div className='space-y-1'>
        <h1 className='font-semibold text-lg'>Something went wrong</h1>
        <p className='max-w-md text-muted-foreground text-sm'>
          This page hit an unexpected error. Your work is safe — reloading usually fixes it.
        </p>
      </div>
      {detail && (
        <pre className='max-w-md overflow-auto rounded-md bg-muted px-3 py-2 text-left text-muted-foreground text-xs'>
          {detail}
        </pre>
      )}
      <div className='flex items-center gap-2'>
        <Button variant='outline' onClick={() => window.location.reload()}>
          Reload page
        </Button>
        <Button
          onClick={() => {
            window.location.href = "/";
          }}
        >
          Go home
        </Button>
      </div>
    </div>
  );
}
