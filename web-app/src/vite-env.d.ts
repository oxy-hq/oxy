/// <reference types="vite/client" />

// Build identifier injected via `define` in vite.config.ts; the same value is
// emitted to /version.json for deploy detection (src/hooks/useVersionCheck.ts).
declare const __APP_VERSION__: string;
