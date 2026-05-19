import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync, openSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WEB_APP_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const LOG_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..", ".logs");
const STARTUP_TIMEOUT_MS = 90_000;
const POLL_INTERVAL_MS = 500;

export interface FrontendHandle {
  url: string;
  spawned: boolean;
  shutdown: () => Promise<void>;
}

export async function ensureFrontend(): Promise<FrontendHandle> {
  // Read `OXY_BASE_URL` lazily — cli.ts may set it after this module
  // imports based on the resolved backend mode. Fall back to the Vite
  // dev-server default when unset.
  const frontendUrl = process.env.OXY_BASE_URL ?? "http://localhost:5173";
  if (await isHealthy(frontendUrl, 3_000)) {
    console.log(`[frontend] using running dev server at ${frontendUrl}`);
    return { url: frontendUrl, spawned: false, shutdown: async () => {} };
  }

  console.log("[frontend] not reachable; starting `pnpm dev` from web-app/");
  mkdirSync(LOG_DIR, { recursive: true });
  const logPath = resolve(LOG_DIR, "frontend.log");
  const out = openSync(logPath, "a");
  const proc = spawn("pnpm", ["dev"], {
    cwd: WEB_APP_DIR,
    env: process.env,
    stdio: ["ignore", out, out],
    detached: false
  });

  const ready = await waitForHealthy(frontendUrl, STARTUP_TIMEOUT_MS);
  if (!ready) {
    proc.kill("SIGTERM");
    throw new Error(
      `frontend did not become healthy within ${STARTUP_TIMEOUT_MS}ms — see ${logPath}`
    );
  }

  console.log(`[frontend] healthy after spawn (logs: ${logPath})`);

  return {
    url: frontendUrl,
    spawned: true,
    shutdown: () => shutdownProc(proc)
  };
}

async function isHealthy(url: string, timeoutMs: number): Promise<boolean> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    return res.ok;
  } catch {
    return false;
  }
}

async function waitForHealthy(url: string, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await isHealthy(url, 2_000)) return true;
    await sleep(POLL_INTERVAL_MS);
  }
  return false;
}

async function shutdownProc(proc: ChildProcess): Promise<void> {
  if (proc.exitCode !== null || proc.killed) return;
  proc.kill("SIGTERM");
  await Promise.race([new Promise<void>((r) => proc.once("exit", () => r())), sleep(5_000)]);
  if (proc.exitCode === null && !proc.killed) proc.kill("SIGKILL");
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
