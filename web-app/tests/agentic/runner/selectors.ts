// Multi-strategy selector materialization + try-with-fallbacks dispatch.
//
// At record time, after a state-changing tool succeeds, we resolve the
// element the LLM clicked/typed and read three durability-graded
// alternatives off the live DOM:
//   testid    — `data-testid` attribute (most stable)
//   role+name — ARIA role + accessible name
//   text      — visible text (≤ 40 chars)
//   css       — the literal selector the LLM emitted (fallback safety net)
//
// At replay time, we walk strategies by rank. The first that resolves +
// dispatches without throwing wins. The runtime's cache layer is
// responsible for re-ranking on a fallback hit (Tier 1 silent heal).
//
// This module is deliberately framework-agnostic on the dispatch side —
// the caller passes a `dispatch` callback so the same machinery works
// for both record-time materialization (Playwright tools) and replay-
// time dispatch (cache replay tools).

import type { Page } from "@playwright/test";
import type { SelectorKind, SelectorStrategy } from "./action-cache";

/**
 * Selector-bearing tools — the ones whose `args.selector` (or
 * `args.element`) names a single page element. We only materialize
 * strategies for these. Other state-changing tools (browser_press_key,
 * browser_navigate) don't have a selector to drift.
 */
const SELECTOR_TOOLS = new Set([
  "browser_click",
  "browser_type",
  "browser_fill",
  "browser_hover",
  "browser_select_option",
  "browser_press_key", // sometimes carries a selector when targeted
  "browser_file_upload"
]);

export function isSelectorTool(name: string): boolean {
  return SELECTOR_TOOLS.has(name);
}

/**
 * Read durability-graded alternative selectors for the element the
 * given args targets. Returns an empty list if the args don't reference
 * a selector or the element can't be resolved on the page (which is
 * normal for non-selector tools like browser_press_key without a
 * `selector` field).
 *
 * The strategy whose `selector` text matches the LLM's primary is bumped
 * to rank 0. If multiple kinds resolve equivalently to the primary
 * (e.g. the LLM already used `[data-testid=foo]`), it's still the rank-0
 * entry — duplicates are de-duped by selector text.
 */
export async function materializeStrategies(
  page: Page,
  toolName: string,
  args: Record<string, unknown>
): Promise<SelectorStrategy[]> {
  if (!isSelectorTool(toolName)) return [];
  const primary = (args.selector ?? args.element) as string | undefined;
  if (!primary || typeof primary !== "string") return [];

  const candidates: Array<{ kind: SelectorKind; selector: string }> = [];
  // Always include the primary so we never lose what the LLM emitted.
  candidates.push({ kind: classifySelector(primary), selector: primary });

  try {
    const locator = page.locator(primary).first();
    // Resolve to a fresh ElementHandle (not memoized — the page may have
    // re-rendered between record and this call). 200ms is long enough
    // for any post-click DOM settle but short enough not to balloon
    // record-time wall clock if the element is gone.
    const handle = await locator.elementHandle({ timeout: 200 }).catch(() => null);
    if (!handle) return uniqueByRank(candidates);

    const testid = await handle.evaluate((el) => el.getAttribute("data-testid")).catch(() => null);
    if (testid) {
      candidates.unshift({ kind: "testid", selector: `[data-testid="${testid}"]` });
    }

    const role = await handle
      .evaluate((el) => {
        const explicit = el.getAttribute("role");
        if (explicit) return explicit;
        const tag = el.tagName.toLowerCase();
        const map: Record<string, string> = {
          a: "link",
          button: "button",
          input: "textbox",
          textarea: "textbox",
          select: "combobox",
          nav: "navigation",
          main: "main",
          header: "banner",
          footer: "contentinfo",
          aside: "complementary"
        };
        return map[tag] ?? null;
      })
      .catch(() => null);
    const name = await handle
      .evaluate((el) => el.getAttribute("aria-label") ?? el.innerText?.trim() ?? null)
      .catch(() => null);
    if (role && name) {
      const escaped = name.slice(0, 40).replace(/'/g, "\\'");
      candidates.push({ kind: "role_name", selector: `role=${role}[name='${escaped}']` });
    }

    const text = await handle.evaluate((el) => el.innerText?.trim() ?? null).catch(() => null);
    if (text) {
      const trimmed = text.slice(0, 40);
      candidates.push({ kind: "text", selector: `text=${trimmed}` });
    }

    await handle.dispose();
  } catch {
    // Best effort. Materialization failure should never break a successful
    // recording — the primary selector still gets cached as a single
    // strategy, just without fallbacks.
  }

  return uniqueByRank(candidates);
}

function uniqueByRank(
  candidates: Array<{ kind: SelectorKind; selector: string }>
): SelectorStrategy[] {
  // Preference order independent of the LLM's primary: testid > role_name
  // > text > css. The primary stays at rank 0 if it doesn't already have
  // a more-stable version (e.g. the LLM used `text=Save` and the element
  // also has a testid → testid is rank 0, text= is rank 1, primary is
  // not duplicated).
  const order: Record<SelectorKind, number> = { testid: 0, role_name: 1, text: 2, css: 3 };
  const seen = new Set<string>();
  const unique = candidates.filter((c) => {
    if (seen.has(c.selector)) return false;
    seen.add(c.selector);
    return true;
  });
  unique.sort((a, b) => order[a.kind] - order[b.kind]);
  return unique.map((c, i) => ({ kind: c.kind, selector: c.selector, rank: i }));
}

export function classifySelector(selector: string): SelectorKind {
  if (/\[data-testid[=~]/.test(selector)) return "testid";
  if (/^role=/.test(selector)) return "role_name";
  if (/^text=/.test(selector)) return "text";
  return "css";
}

/**
 * Try `args.selector`, then walk fallback strategies by rank. Returns
 * the strategy index that succeeded (0 = primary). Throws when every
 * strategy fails — caller decides whether to invalidate the cache and
 * trigger a Tier 2 healing redrive.
 */
export async function dispatchWithFallbacks<R>(
  args: Record<string, unknown>,
  strategies: SelectorStrategy[] | undefined,
  dispatch: (args: Record<string, unknown>) => Promise<R>
): Promise<{ result: R; strategyIndex: number; usedSelector: string; usedKind: SelectorKind }> {
  // No strategies → behave like the v2 single-selector replay.
  if (!strategies || strategies.length === 0) {
    const result = await dispatch(args);
    const sel = (args.selector ?? args.element) as string | undefined;
    return {
      result,
      strategyIndex: 0,
      usedSelector: sel ?? "",
      usedKind: sel ? classifySelector(sel) : "css"
    };
  }

  const sorted = [...strategies].sort((a, b) => a.rank - b.rank);
  let lastErr: unknown;
  for (let i = 0; i < sorted.length; i++) {
    const strategy = sorted[i];
    try {
      const swapped: Record<string, unknown> = { ...args };
      if ("selector" in swapped) swapped.selector = strategy.selector;
      if ("element" in swapped) swapped.element = strategy.selector;
      const result = await dispatch(swapped);
      return {
        result,
        strategyIndex: i,
        usedSelector: strategy.selector,
        usedKind: strategy.kind
      };
    } catch (err) {
      lastErr = err;
    }
  }
  throw lastErr instanceof Error ? lastErr : new Error("all selector strategies failed for action");
}
