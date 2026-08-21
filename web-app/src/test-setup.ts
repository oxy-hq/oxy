import "@testing-library/jest-dom/vitest";

// Radix observes its triggers (Tooltip, Popover, Select), and jsdom has no
// ResizeObserver — without this a component test renders but its trigger's
// activation never runs, which makes a test pass for the wrong reason rather
// than fail loudly. Shared here so the next file to render Radix does not have
// to rediscover it.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;
