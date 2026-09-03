/**
 * Registering a customer is TAGGING A REPO. This file is the only thing that
 * writes that tag.
 *
 * `customers.ts` answers "who are the customers" by reading the topic; this is
 * the other half. The topic name is NOT redefined here — it comes from
 * `customersTopic()`, so the reader and the writer cannot disagree about what
 * registration means.
 *
 * THE RULE THIS FILE IS SHAPED BY, inverted from `customers.ts`'s:
 *
 *     A 404 is an answer. Offline, 5xx, rate-limited, an expired token, or a
 *     private repo you cannot see are INCONCLUSIVE — never "missing".
 *
 * It matters more here than anywhere else because the caller ACTS on it:
 * `import` refuses a repo that does not exist, and `new` CREATES one it
 * believes does not exist. Reading "GitHub is having a moment" as "no such
 * repo" is how `new` clobbers a live customer's repository.
 */

import { CliError, ExitCode } from "../util/errors.js";
import { customersTopic, invalidateCache } from "./customers.js";
import { ghJson, ghRaw } from "./gh.js";

/** What we could learn about a repo's existence. */
export type RepoExistence =
  | { kind: "exists"; topics: string[]; description: string; isPrivate: boolean }
  | { kind: "absent" }
  | { kind: "inconclusive"; why: string };

/**
 * Does `<org>/<name>` exist, and what is on it?
 *
 * The three-way return is the whole design. A boolean would force every caller
 * to pick a side for "we could not tell", and both sides are wrong: treating
 * it as present blocks a legitimate `new`, treating it as absent lets `new`
 * create over a repo that is merely invisible right now.
 */
export function inspectRepo(slug: string): RepoExistence {
  const result = ghRaw([
    "repo",
    "view",
    slug,
    "--json",
    "name,description,isPrivate,repositoryTopics"
  ]);
  if (result.status === 0) {
    try {
      const parsed = JSON.parse(result.stdout) as {
        description?: string;
        isPrivate?: boolean;
        repositoryTopics?: Array<{ name?: string }> | null;
      };
      return {
        kind: "exists",
        topics: (parsed.repositoryTopics ?? []).map((t) => t.name ?? "").filter(Boolean),
        description: parsed.description ?? "",
        isPrivate: Boolean(parsed.isPrivate)
      };
    } catch {
      return { kind: "inconclusive", why: "gh returned unparsable JSON for the repo" };
    }
  }

  const stderr = result.stderr.toLowerCase();
  // Only these two spellings are a real 404. Everything else — including
  // "HTTP 403" on a private repo the token cannot see — is inconclusive.
  if (stderr.includes("could not resolve to a repository") || stderr.includes("404")) {
    return { kind: "absent" };
  }
  return { kind: "inconclusive", why: result.stderr.trim() || `gh exited ${result.status}` };
}

/** Turn an inconclusive answer into the refusal it has to be. */
export function refuseInconclusive(slug: string, why: string): CliError {
  return new CliError(`could not determine whether ${slug} exists`, {
    code: ExitCode.REFUSED,
    detail: why,
    hint: "retry once GitHub is reachable — this is NOT 'the repo does not exist'"
  });
}

/** Is this repo registered as a customer workspace? */
export function isRegistered(slug: string): boolean {
  const repo = inspectRepo(slug);
  if (repo.kind === "inconclusive") throw refuseInconclusive(slug, repo.why);
  if (repo.kind === "absent") return false;
  return repo.topics.includes(customersTopic());
}

/**
 * Add the topic. Idempotent, because `gh repo edit --add-topic` is.
 *
 * The cache is invalidated on the way out rather than left to expire: the
 * whole point of `import` is that the next command sees the customer, and an
 * hour of "unknown customer" after a successful registration reads as a
 * failed registration.
 */
export function addTopic(slug: string): void {
  const result = ghRaw(["repo", "edit", slug, "--add-topic", customersTopic()]);
  if (result.status !== 0) {
    throw new CliError(`could not tag ${slug} as a customer workspace`, {
      code: ExitCode.FAILURE,
      detail: result.stderr.trim() || undefined,
      hint: "you need admin rights on the repo to edit its topics"
    });
  }
  invalidateCache();
}

/** Drop the topic. The REPO is never deleted — see `oxyc remove`. */
export function removeTopic(slug: string): void {
  const result = ghRaw(["repo", "edit", slug, "--remove-topic", customersTopic()]);
  if (result.status !== 0) {
    throw new CliError(`could not untag ${slug}`, {
      code: ExitCode.FAILURE,
      detail: result.stderr.trim() || undefined
    });
  }
  invalidateCache();
}

/** Set the repo description, which is what `oxyc list` shows as a display name. */
export function setDescription(slug: string, description: string): void {
  const result = ghRaw(["repo", "edit", slug, "--description", description]);
  if (result.status !== 0) {
    throw new CliError(`could not set the description on ${slug}`, {
      code: ExitCode.FAILURE,
      detail: result.stderr.trim() || undefined
    });
  }
  invalidateCache();
}

/**
 * Create the repo, already tagged and private.
 *
 * Private is not a default worth making configurable: these repos hold a
 * customer's semantic model and their memory facts. A public one is a
 * disclosure, and the flag to make it public would exist only to be set by
 * accident.
 */
export function createRepo(slug: string, description: string): void {
  const args = ["repo", "create", slug, "--private", "--description", description];
  const result = ghRaw(args);
  if (result.status !== 0) {
    throw new CliError(`could not create ${slug}`, {
      code: ExitCode.FAILURE,
      detail: result.stderr.trim() || undefined
    });
  }
  addTopic(slug);
}

/** The default branch, for the commands that must not act on a detached HEAD. */
export function defaultBranch(slug: string): string | undefined {
  try {
    const parsed = ghJson<{ defaultBranchRef?: { name?: string } }>(
      ["repo", "view", slug, "--json", "defaultBranchRef"],
      `${slug}'s default branch`
    );
    return parsed.defaultBranchRef?.name;
  } catch {
    return undefined;
  }
}
