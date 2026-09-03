/**
 * `oxyc activity <customer>` — merged pull requests for a customer, from their
 * own repo and from ours.
 *
 * Reporting is the default; `--write` files the record into the customer's
 * dossier. Nothing is committed: the record rides your pull request like any
 * other change.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Context } from "../context/resolve.js";
import {
  collectActivity,
  monthsIn,
  renderJsonl,
  renderMonth,
  requireNonEmpty
} from "../customer/activity.js";
import { dossierPath, isCloned } from "../customer/dossier.js";
import { customersOrg, displayName, resolveCustomer } from "../github/customers.js";
import * as log from "../ui/log.js";
import { table } from "../ui/render.js";
import { out } from "../ui/tty.js";
import { CliError, ExitCode } from "../util/errors.js";

export interface ActivityFlags {
  since?: string;
  repo: string[];
  write?: boolean;
  json?: boolean;
}

export function runActivity(ctx: Context, name: string, flags: ActivityFlags): void {
  const customer = resolveCustomer(name, { refresh: ctx.flags.refresh });
  const org = customersOrg();
  const slug = `${org}/${customer.name}`;
  const display = displayName(customer);

  if (flags.since && !/^\d{4}-\d{2}-\d{2}$/.test(flags.since)) {
    throw new CliError(`--since expects YYYY-MM-DD, got "${flags.since}"`, {
      code: ExitCode.USAGE
    });
  }

  const records = collectActivity({
    name: customer.name,
    ownRepo: slug,
    since: flags.since,
    extraRepos: flags.repo
  });

  if (flags.json) {
    process.stdout.write(`${JSON.stringify(records, null, 2)}\n`);
    return;
  }

  requireNonEmpty(records, customer.name);

  if (!flags.write) {
    process.stdout.write(
      `${table(records, [
        { header: "MONTH", value: (r) => r.month },
        { header: "REPO", value: (r) => r.repo },
        { header: "PR", value: (r) => `#${r.number}`, align: "right" },
        { header: "TITLE", value: (r) => r.title },
        { header: "VIA", value: (r) => r.via }
      ])}\n`
    );
    return;
  }

  if (!isCloned(slug)) {
    throw new CliError(`${slug} is not cloned here, so there is nowhere to write the record`, {
      code: ExitCode.NOT_FOUND,
      hint: `gh repo clone ${slug} ${dossierPath(slug)}`
    });
  }

  const dir = join(dossierPath(slug), "activity");
  mkdirSync(dir, { recursive: true });
  const written: string[] = [];
  // A month with no merged PRs never appears in `monthsIn`, so it produces no
  // file rather than an empty one — an empty file is a claim.
  for (const month of monthsIn(records)) {
    const md = join(dir, `${month}.md`);
    const jsonl = join(dir, `${month}.jsonl`);
    writeFileSync(md, `${renderMonth(month, display, records)}\n`);
    writeFileSync(jsonl, `${renderJsonl(month, records)}\n`);
    written.push(md, jsonl);
  }

  process.stdout.write(`${out.green(`wrote ${written.length} file(s) under ${dir}`)}\n`);
  log.info("nothing was committed — the record rides your pull request like any other change");
}
