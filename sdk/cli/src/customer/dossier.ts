/**
 * Where a customer's repo lives on THIS machine.
 *
 * A customer IS an `<org>/<name>` repo on GitHub — shared with the team, so
 * nothing about their identity may be a path that exists on one laptop. The
 * local path is DERIVED, per machine, from the slug:
 *
 *     <root>/<org>/<name>,  root = $OXYC_DOSSIER_ROOT or ~/.oxyc/dossiers
 *
 * The default root is tool-owned, like `~/.claude` or `~/.cargo`. A default
 * under the user's own code tree would fix portability across usernames while
 * still assuming every teammate lays their code out the same way — the same
 * class of assumption this scheme exists to remove. `$OXYC_DOSSIER_ROOT` is
 * the escape hatch for anyone who wants dossiers in their own layout; the
 * clones stay ordinary git repos wherever they land.
 *
 * The root deliberately stops BEFORE the org: the org comes from the slug, so
 * two orgs cannot collide on a repo name, and a root that already carried one
 * would resolve `oxy-hq/acme-oxy` to `…/oxy-hq/oxy-hq/acme-oxy`.
 */

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { ghAvailable } from "../github/gh.js";
import * as log from "../ui/log.js";
import { CliError, ExitCode } from "../util/errors.js";
import { clone, looksLikeCheckout, repoRoot, slugFromRemote } from "../util/git.js";

/**
 * The directory every dossier clone sits under.
 *
 * `~/.oxyc/dossiers` rather than this tool's own state directory, and that is
 * a MIGRATION decision, not a preference: the bash `oxyc` this replaces used
 * exactly that path, and teammates have customer repos cloned there with
 * branches and uncommitted work in them. Moving the default would leave every
 * one of those orphaned — the tool would report each customer as "not cloned
 * here" and cheerfully clone a second copy beside the first.
 */
export function dossierRoot(): string {
  return process.env.OXYC_DOSSIER_ROOT ?? join(homedir(), ".oxyc", "dossiers");
}

/**
 * The local path a repo slug resolves to.
 *
 * The slug is VALIDATED rather than pasted in: `<root>/<whatever>` would
 * happily accept an absolute path (`/Users/someone/…` → `<root>//Users/…`) or
 * a `..`, which is exactly the machine-specific shape this scheme exists to
 * keep out of a customer's identity.
 */
export function dossierPath(slug: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]*\/[A-Za-z0-9][A-Za-z0-9._-]*$/.test(slug)) {
    throw new CliError(`not a repo slug: "${slug}"`, {
      code: ExitCode.USAGE,
      hint: "expected <org>/<name>, e.g. oxy-hq/acme-oxy — not a path"
    });
  }
  return join(dossierRoot(), slug);
}

/** Is the clone here? */
export function isCloned(slug: string): boolean {
  const path = dossierPath(slug);
  return existsSync(path) && looksLikeCheckout(path);
}

/**
 * The clone, cloning it if it is not here yet.
 *
 * ONE DECISION, TWO CONSUMERS: the command PRINTED when this declines to clone
 * must be the command it would have RUN. Splitting the choice out is what
 * stops the printed instruction from drifting away from the real one.
 */
export function ensureCloned(slug: string, opts: { autoClone?: boolean } = {}): string {
  const path = dossierPath(slug);
  if (isCloned(slug)) return path;

  const useGh = ghAvailable();
  const command = useGh
    ? `gh repo clone ${slug} ${path}`
    : `git clone https://github.com/${slug}.git ${path}`;

  if (!opts.autoClone) {
    throw new CliError(`${slug} is not cloned here`, {
      code: ExitCode.NOT_FOUND,
      hint: command
    });
  }

  log.info(`cloning ${slug} → ${path}`);
  clone(slug, path, useGh);
  return path;
}

/**
 * The customer a directory belongs to, if any.
 *
 * Resolves by walking to the git root and reading `origin`, so it works from
 * anywhere inside the checkout — including from inside a subdirectory
 * workspace, which is where a session that has `cd`-ed into `oxy/` will be.
 */
export function slugForDirectory(dir: string): string | undefined {
  const root = repoRoot(dir);
  if (!root) return undefined;
  return slugFromRemote(root);
}
