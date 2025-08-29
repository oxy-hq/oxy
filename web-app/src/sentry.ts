// Sentry initialization for web-app
import * as Sentry from "@sentry/react";

const SENTRY_DSN = import.meta.env.VITE_SENTRY_DSN;
const SENTRY_ENV = import.meta.env.VITE_SENTRY_ENV;

export function initSentry() {
  if (!SENTRY_DSN || SENTRY_DSN === '""' || SENTRY_DSN === "") {
    // Sentry DSN not provided, do not initialize
    return;
  }
  Sentry.init({
    dsn: SENTRY_DSN,
    environment: SENTRY_ENV || "production",
  });
}
