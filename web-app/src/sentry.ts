import * as Sentry from "@sentry/react";

const SENTRY_DSN = import.meta.env.VITE_SENTRY_DSN;
const SENTRY_ENV = import.meta.env.VITE_SENTRY_ENV;
const SENTRY_RELEASE = import.meta.env.VITE_SENTRY_RELEASE || import.meta.env.VITE_APP_VERSION;
const SENTRY_TRACES_SAMPLE_RATE = Number(import.meta.env.VITE_SENTRY_TRACES_SAMPLE_RATE ?? "0.0");

function isDsnDefined(dsn: unknown): dsn is string {
  return typeof dsn === "string" && dsn.length > 0 && dsn !== '""';
}

export function initSentry() {
  if (!isDsnDefined(SENTRY_DSN)) return;

  const integrations = [
    Sentry.consoleLoggingIntegration({ levels: ["log", "error", "warn"] }),
    Sentry.browserTracingIntegration?.(),
    Sentry.replayIntegration?.(),
    Sentry.linkedErrorsIntegration?.({ limit: 5 }),
    Sentry.reportingObserverIntegration?.(),
    Sentry.captureConsoleIntegration?.({ levels: ["error", "warn"] })
  ].filter(Boolean);

  type BeforeSendType = Parameters<typeof Sentry.init>[0]["beforeSend"];

  // disable unused-var warning for the unused _hint parameter
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const beforeSend: BeforeSendType = (event, _hint) => {
    try {
      if (event?.request) {
        const req = event.request as { data?: unknown } & Record<string, unknown>;
        if (Object.hasOwn(req, "data")) {
          delete req.data;
        }
      }
    } catch {
      // ignore
    }

    try {
      const values = event.exception?.values;
      if (Array.isArray(values)) {
        event.exception!.values = values.filter((v) => {
          const msg = v?.value ?? "";
          return !(typeof msg === "string" && msg.includes("ResizeObserver loop limit exceeded"));
        });
      }
    } catch {
      // ignore
    }

    return event;
  };

  Sentry.init({
    dsn: SENTRY_DSN,
    environment: SENTRY_ENV || "production",
    release: SENTRY_RELEASE,
    integrations: integrations.length ? integrations : undefined,
    tracesSampleRate: Math.min(Math.max(SENTRY_TRACES_SAMPLE_RATE, 0), 1),
    beforeSend
  });
}

// Re-export ErrorBoundary for convenience
export const ErrorBoundary = Sentry.ErrorBoundary;
export default Sentry;
