/**
 * Send every request through the proxy the environment names.
 *
 * Node's `fetch` ignores `HTTPS_PROXY` unless `NODE_USE_ENV_PROXY=1` was set
 * before the process started. Where a proxy is the only way out — a Claude
 * Code cloud session, an agent sandbox, a corporate network — every request
 * failed with a bare `fetch failed` (exit 7) and nothing said why.
 *
 * Three ways to fix it, best first:
 *   - `http.setGlobalProxyFromEnv()` turns the support on inside this process
 *     (Node 25+);
 *   - a Node that reads `NODE_USE_ENV_PROXY` but has no in-process switch
 *     (22.21+, 24.5+ — what a Claude cloud session runs) gets oxyc re-run
 *     once with the variable set (`relaunchWithEnvProxy`);
 *   - older Node cannot proxy `fetch` at all, so the transport error says so
 *     (`proxyHint`).
 *
 * The standalone binaries run on Bun, whose `fetch` reads the variables itself.
 */

import { spawnSync } from "node:child_process";
import * as http from "node:http";
import { constants } from "node:os";

const PROXY_VARIABLES = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];

/**
 * What `routeThroughEnvProxy` found:
 *   `none`         no proxy variable is set
 *   `runtime`      the runtime already routes through it
 *   `enabled`      this process turned it on
 *   `relaunch`     only a process started with `NODE_USE_ENV_PROXY=1` will
 *   `unsupported`  this Node cannot route `fetch` through a proxy
 */
export type ProxySetup = "none" | "runtime" | "enabled" | "relaunch" | "unsupported";

let setup: ProxySetup = "none";

type HttpModule = { setGlobalProxyFromEnv?: (env?: NodeJS.ProcessEnv) => unknown };

export interface Runtime {
  node: string;
  bun?: string;
}

/** Call once, before the first request. */
export function routeThroughEnvProxy(
  env: NodeJS.ProcessEnv = process.env,
  mod: HttpModule = http as HttpModule,
  runtime: Runtime = process.versions
): ProxySetup {
  setup = decide(env, mod, runtime);
  return setup;
}

function decide(env: NodeJS.ProcessEnv, mod: HttpModule, runtime: Runtime): ProxySetup {
  if (!PROXY_VARIABLES.some((name) => env[name]?.trim())) return "none";
  if (runtime.bun) return "runtime";
  const reads = readsEnvProxyVariable(runtime.node);
  // Set on a Node that ignores it (a session hook sets it everywhere), the
  // variable changes nothing, and the error must still say why.
  if (env.NODE_USE_ENV_PROXY === "1") return reads ? "runtime" : "unsupported";
  if (typeof mod.setGlobalProxyFromEnv === "function") {
    mod.setGlobalProxyFromEnv(env);
    return "enabled";
  }
  return reads ? "relaunch" : "unsupported";
}

/** Whether this Node version honours `NODE_USE_ENV_PROXY=1` (22.21+, 24.5+). */
export function readsEnvProxyVariable(version: string): boolean {
  const [major = 0, minor = 0] = version.split(".").map(Number);
  if (major === 22) return minor >= 21;
  if (major === 24) return minor >= 5;
  return major >= 25;
}

/**
 * Run this same command again with `NODE_USE_ENV_PROXY=1`, wait for it, and
 * return the exit code to leave with. The child decides `runtime`, so it
 * never relaunches in turn.
 */
export function relaunchWithEnvProxy(): number {
  const result = spawnSync(process.execPath, [...process.execArgv, ...process.argv.slice(1)], {
    stdio: "inherit",
    env: { ...process.env, NODE_USE_ENV_PROXY: "1" }
  });
  if (result.error) throw result.error;
  if (result.signal) return 128 + (constants.signals[result.signal] ?? 0);
  return result.status ?? 1;
}

/** The fix for a request that never left, when the proxy is why. */
export function proxyHint(): string | undefined {
  if (setup !== "unsupported") return undefined;
  return `a proxy is set (HTTPS_PROXY) and Node ${process.version} cannot send fetch through one — use Node 22.21+ or 24.5+`;
}
