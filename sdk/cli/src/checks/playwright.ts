/**
 * Playwright, found on demand for `oxyc checks run --browser`.
 *
 * NOT a dependency of `@oxy-hq/cli`: a browser driver is tens of megabytes
 * before the browser itself, and every other verb would pay for it. The check
 * resolves whichever Playwright package the caller already has — an app's own
 * dev dependency, the repo's `web-app`, a global install — the way the
 * create-oxy-app templates' `screenshot.mjs` does, and says how to add one when
 * none resolves.
 *
 * The types below are the slice of Playwright's API the check calls, declared
 * structurally so this package compiles without it.
 */

import { createRequire } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { CliError, ExitCode } from "../util/errors.js";

export interface PwRequest {
  url(): string;
  headers(): Record<string, string>;
  postData(): string | null;
}

export interface PwResponse {
  url(): string;
  status(): number;
  headers(): Record<string, string>;
}

export interface PwRoute {
  request(): PwRequest;
  continue(opts?: { headers?: Record<string, string> }): Promise<void>;
}

export interface PwLocator {
  first(): PwLocator;
  waitFor(opts: { state: "visible"; timeout: number }): Promise<void>;
}

export interface PwPage {
  on(event: "pageerror", fn: (error: Error) => void): void;
  on(event: "response", fn: (response: PwResponse) => void): void;
  waitForRequest(
    pred: (request: PwRequest) => boolean,
    opts: { timeout: number }
  ): Promise<PwRequest>;
  goto(url: string, opts: { timeout: number }): Promise<PwResponse | null>;
  url(): string;
  locator(selector: string): PwLocator;
  waitForLoadState(state: "networkidle", opts: { timeout: number }): Promise<void>;
  evaluate<T>(fn: () => T): Promise<T>;
}

export interface PwContext {
  route(pattern: string, handler: (route: PwRoute) => Promise<void>): Promise<void>;
  newPage(): Promise<PwPage>;
}

export interface PwBrowser {
  newContext(opts: { serviceWorkers: "block" }): Promise<PwContext>;
  close(): Promise<void>;
}

export interface PwChromium {
  launch(opts: { headless: boolean }): Promise<PwBrowser>;
}

/** Tried in this order from each base; the first that exports `chromium` wins. */
export const PLAYWRIGHT_PACKAGES = ["playwright", "playwright-core", "@playwright/test"] as const;

const INSTALL_REMEDY =
  "pnpm add -D playwright && pnpm exec playwright install chromium — in the app's directory or any above it";

/**
 * `chromium` from the first Playwright package that resolves from `cwd`, then
 * from this CLI's own install. A module that resolves but exports no
 * `chromium` is skipped rather than trusted.
 */
export async function loadChromium(cwd: string = process.cwd()): Promise<PwChromium> {
  const bases = [join(cwd, "package.json"), import.meta.url];
  for (const base of bases) {
    const resolver = createRequire(base);
    for (const name of PLAYWRIGHT_PACKAGES) {
      let resolved: string;
      try {
        resolved = resolver.resolve(name);
      } catch {
        continue;
      }
      const mod = (await import(pathToFileURL(resolved).href)) as {
        chromium?: PwChromium;
        default?: { chromium?: PwChromium };
      };
      const chromium = mod.chromium ?? mod.default?.chromium;
      if (typeof chromium?.launch === "function") return chromium;
    }
  }
  throw new CliError("the browser check needs Playwright, and none resolves from here", {
    code: ExitCode.USAGE,
    hint: `looked for ${PLAYWRIGHT_PACKAGES.join(", ")} from ${cwd} and from oxyc's own install`,
    remedy: INSTALL_REMEDY
  });
}

/** Launch headless Chromium, turning a missing browser binary into the install remedy. */
export async function launchChromium(chromium: PwChromium): Promise<PwBrowser> {
  try {
    return await chromium.launch({ headless: true });
  } catch (cause) {
    const message = (cause as Error).message ?? String(cause);
    if (/Executable doesn't exist|playwright install/i.test(message)) {
      throw new CliError("Playwright resolves, but its Chromium is not installed", {
        code: ExitCode.USAGE,
        remedy: "pnpm exec playwright install chromium"
      });
    }
    throw new CliError(`could not start Chromium: ${message.split("\n")[0]}`, {
      code: ExitCode.FAILURE
    });
  }
}
