// Diagnostic logger for custom-app bundles.
//
// Customer apps are built by our internal team; "open DevTools, read
// the logs" is a real debugging workflow. The SDK logs every fetch
// lifecycle (start, success, error) with structured context so an
// internal dev can correlate UI behavior with what hit the wire
// without needing server logs.
//
// Defaults to console at info level with a `[oxy-app]` prefix.
// Override via `setOxyAppLogger(...)` for tests or production silence.
//
// Log lines are formatted as:
//   [oxy-app] <event> { …structured ctx… }
// so DevTools' object inspector unfolds them.

export type OxyAppLogLevel = "debug" | "info" | "warn" | "error";

export interface OxyAppLogger {
  log(level: OxyAppLogLevel, msg: string, ctx?: Record<string, unknown>): void;
}

let activeLogger: OxyAppLogger = createConsoleLogger();

/** Replace the global logger. Pass `null` to silence everything. */
export function setOxyAppLogger(logger: OxyAppLogger | null): void {
  activeLogger = logger ?? silentLogger();
}

/** Used by the SDK internals; not part of the public surface. */
export function getOxyAppLogger(): OxyAppLogger {
  return activeLogger;
}

function createConsoleLogger(): OxyAppLogger {
  return {
    log(level, msg, ctx) {
      if (typeof console === "undefined") return;
      const prefix = "[oxy-app]";
      const args: unknown[] = ctx ? [prefix, msg, ctx] : [prefix, msg];
      switch (level) {
        case "debug":
          console.debug(...args);
          break;
        case "info":
          console.info(...args);
          break;
        case "warn":
          console.warn(...args);
          break;
        case "error":
          console.error(...args);
          break;
      }
    }
  };
}

function silentLogger(): OxyAppLogger {
  return { log() {} };
}
