/**
 * `oxyc repos` — where OUR repos are checked out on this machine.
 *
 * Read-only, and it never clones: a repo that is not here is named with the
 * command that would fetch it, and the human decides.
 */

import { repoMap, SHARED_REPOS } from "../customer/repos.js";
import { table } from "../ui/render.js";
import { out } from "../ui/tty.js";

export function runRepos(flags: { refresh?: boolean }): void {
  const map = repoMap({ refresh: flags.refresh });
  // The shared four first, in their canonical order, then anything else the
  // scan turned up — a machine that also holds `oxy-hq/docs` should say so,
  // but not ahead of the repos a customer session actually reaches for.
  const rows = [
    ...SHARED_REPOS.map((slug) => ({ slug, path: map[slug], shared: true })),
    ...Object.entries(map)
      .filter(([slug]) => !SHARED_REPOS.includes(slug as (typeof SHARED_REPOS)[number]))
      .map(([slug, path]) => ({ slug, path, shared: false }))
  ];

  process.stdout.write(
    `${table(rows, [
      { header: "REPO", value: (r) => r.slug },
      { header: "SHARED", value: (r) => (r.shared ? "yes" : "") },
      {
        header: "PATH",
        value: (r) => r.path ?? out.dim(`not on this machine — gh repo clone ${r.slug}`)
      }
    ])}\n`
  );
}
