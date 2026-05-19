import type { Page } from "@playwright/test";

const MAX_SNAPSHOT_BYTES = 12_000;

export interface SnapshotOptions {
  /**
   * Optional region to scope the snapshot to.
   *   - omitted / undefined → full `body` (default, preserves backward
   *     compatibility with steps that don't pass a region).
   *   - `"main"` → the page's main content region (`role=main` or `<main>`),
   *     falling back to `body` if no main landmark exists.
   *   - any other string → treated as a CSS selector for the subtree.
   *
   * Region scoping is useful on flows that work inside one panel of a
   * multi-pane UI (e.g. the IDE's editor). It can roughly halve snapshot
   * bytes when most of the page is irrelevant to the step.
   */
  region?: string;
}

// Returns Playwright's AI-optimized aria snapshot of the chosen region. The
// format is YAML-ish: each line is "- role 'name' [state]" and children are
// indented. We truncate to ~12kB so a sprawling page (e.g. the IDE file
// tree) doesn't blow the LLM context. If the snapshot is truncated, the
// caller can fall back to browser_get_page_text.
export async function captureSnapshot(page: Page, opts: SnapshotOptions = {}): Promise<string> {
  const target = await resolveTarget(page, opts.region);
  const snap = await target
    .ariaSnapshot({ mode: "ai", timeout: 5_000 })
    .catch((err: unknown) => `(snapshot failed: ${formatErr(err)})`);

  if (!snap) return "(empty snapshot)";
  if (snap.length <= MAX_SNAPSHOT_BYTES) return snap;
  return `${snap.slice(0, MAX_SNAPSHOT_BYTES)}\n... (truncated; call browser_get_page_text for the rest)`;
}

async function resolveTarget(page: Page, region?: string) {
  if (!region) return page.locator("body");
  if (region === "main") {
    const main = page.locator('[role="main"], main').first();
    const count = await main.count().catch(() => 0);
    return count > 0 ? main : page.locator("body");
  }
  return page.locator(region).first();
}

function formatErr(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
