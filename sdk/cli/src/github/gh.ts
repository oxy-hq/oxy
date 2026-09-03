/**
 * Every call to the `gh` CLI goes through here.
 *
 * THE SAFETY RULE, which this whole file exists to enforce, is inherited from
 * `customer-tooling/lib/customers.sh` and is worth restating because every
 * function below is shaped by it:
 *
 *     An empty or partial answer is an ERROR, never "there is nothing".
 *
 * GitHub is the only thing that knows who the customers are. A `gh` that is
 * missing, unauthenticated, rate-limited, offline or 5xx-ing must fail LOUDLY,
 * because an empty list returned from any of those makes every customer
 * silently vanish — from `oxyc list`, from name resolution, from every command
 * that asks. Nobody audits a list for absence.
 *
 * The inverse rule lives in `topics.ts`, for the write path: a 404 IS an
 * answer; everything else is inconclusive.
 */

import { spawnSync } from "node:child_process";

import { CliError, ExitCode } from "../util/errors.js";

export interface GhResult {
  stdout: string;
  stderr: string;
  status: number;
}

/** Run `gh` and hand back the raw result. Never throws on a non-zero exit. */
export function ghRaw(args: string[], input?: string): GhResult {
  const result = spawnSync("gh", args, {
    input,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024
  });
  if (result.error) {
    const missing = (result.error as NodeJS.ErrnoException).code === "ENOENT";
    throw new CliError(
      missing ? "the `gh` CLI is not installed" : `gh failed to start: ${result.error.message}`,
      {
        code: missing ? ExitCode.USAGE : ExitCode.FAILURE,
        hint: missing ? "brew install gh && gh auth login" : undefined
      }
    );
  }
  return {
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    status: result.status ?? 1
  };
}

/**
 * Classify a `gh` failure into a message that says which of the four things
 * went wrong.
 *
 * Four causes, four messages, because only one of them is about the data and
 * the other three are about the caller's machine. "no customers found" for an
 * expired token is the specific wrong answer this replaces.
 */
function classify(result: GhResult, what: string): CliError {
  const stderr = result.stderr.toLowerCase();
  if (
    stderr.includes("not logged") ||
    stderr.includes("authentication") ||
    stderr.includes("gh auth login")
  ) {
    return new CliError(`not authenticated to GitHub, so ${what} cannot be read`, {
      code: ExitCode.AUTH,
      hint: "gh auth login"
    });
  }
  if (stderr.includes("rate limit")) {
    return new CliError(`GitHub rate-limited the request for ${what}`, {
      code: ExitCode.UNAVAILABLE,
      hint: "wait for the limit to reset, or use a token with a higher quota"
    });
  }
  if (
    stderr.includes("dial tcp") ||
    stderr.includes("no such host") ||
    stderr.includes("connection refused") ||
    stderr.includes("timeout") ||
    stderr.includes("network is unreachable")
  ) {
    return new CliError(`could not reach GitHub to read ${what}`, {
      code: ExitCode.UNAVAILABLE,
      hint: "check your connection and retry"
    });
  }
  return new CliError(`gh failed while reading ${what}`, {
    code: ExitCode.UNAVAILABLE,
    detail: result.stderr.trim() || undefined
  });
}

/**
 * Run `gh` and parse JSON, refusing every shape of "nothing came back".
 *
 * `what` names the thing being read so the four messages above can be
 * specific — "the customer list", "pokehouse-oxy's topics".
 *
 * An exit-0-with-no-stdout is refused BEFORE parsing, because `JSON.parse("")`
 * throws while `gh` exiting 0 with empty output is a real failure mode
 * (a broken pipe, an aborted paginated call) that would otherwise reach the
 * caller as a parse error about the wrong thing.
 */
export function ghJson<T>(args: string[], what: string): T {
  const result = ghRaw(args);
  if (result.status !== 0) throw classify(result, what);
  if (!result.stdout.trim()) {
    throw new CliError(`gh returned nothing while reading ${what}`, {
      code: ExitCode.UNAVAILABLE,
      hint: "this is not an empty result — retry, and check `gh auth status`"
    });
  }
  try {
    return JSON.parse(result.stdout) as T;
  } catch (cause) {
    throw new CliError(`could not parse gh's answer for ${what}`, {
      code: ExitCode.UNAVAILABLE,
      detail: (cause as Error).message
    });
  }
}

/**
 * A listing that came back exactly AT its limit is refused, not served.
 *
 * `gh repo list` and `gh search prs` truncate silently at their limit, and a
 * truncated list is a partial one — its dropped rows look like customers who
 * were deleted, or work nobody did. The guard cannot tell "exactly N,
 * complete" from "truncated at N", so it refuses both and says how to widen.
 *
 * This is not hypothetical: the first real `oxyc activity pokehouse` refused
 * because that repo has exactly 200 merged PRs and the limit was 200.
 */
export function refuseIfAtLimit(count: number, limit: number, what: string): void {
  if (count < limit) return;
  throw new CliError(`${what} came back at the limit of ${limit}, so it may be truncated`, {
    code: ExitCode.REFUSED,
    hint: `narrow the query, or raise the limit — a truncated list is served as complete otherwise`
  });
}

/** Whether `gh` is on PATH at all. For `oxyc doctor`, which reports rather than fails. */
export function ghAvailable(): boolean {
  const result = spawnSync("gh", ["--version"], { encoding: "utf8" });
  return !result.error && result.status === 0;
}
