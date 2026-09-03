/**
 * `oxyc list` and `oxyc path` — the two commands that answer "who is there"
 * and "where is their repo".
 */

import type { Context } from "../context/resolve.js";
import { dossierPath, isCloned } from "../customer/dossier.js";
import { customersOrg, displayName, listCustomers, resolveCustomer } from "../github/customers.js";
import { table } from "../ui/render.js";
import { out } from "../ui/tty.js";

/**
 * The customers, with their display names and whether their repo is here.
 *
 * `--refresh` is on THIS command specifically, and it is the only way past the
 * hour-long cache. That placement is deliberate: a customer a teammate
 * registered minutes ago is invisible to every other command until the cache
 * expires, so every "unknown customer" message points here.
 */
export function runList(_ctx: Context, flags: { refresh?: boolean; json?: boolean }): void {
  const org = customersOrg();
  const customers = listCustomers({ refresh: flags.refresh });

  if (flags.json) {
    process.stdout.write(
      `${JSON.stringify(
        customers.map((c) => ({
          name: c.name,
          display_name: displayName(c),
          slug: `${org}/${c.name}`,
          cloned: isCloned(`${org}/${c.name}`),
          path: dossierPath(`${org}/${c.name}`)
        })),
        null,
        2
      )}\n`
    );
    return;
  }

  if (customers.length === 0) {
    // Reaching here means the listing SUCCEEDED and was empty — every failure
    // mode threw upstream. So this really is "nobody is tagged", and saying
    // which topic decides it is the actionable half.
    process.stderr.write(
      `${out.yellow(`no repos in ${org} carry the customer topic.`)}\n` +
        `  ${out.dim(`register one with \`oxyc import ${org}/<repo>\``)}\n`
    );
    return;
  }

  process.stdout.write(
    `${table(customers, [
      { header: "CUSTOMER", value: (c) => c.name },
      { header: "NAME", value: (c) => displayName(c) },
      { header: "LOCAL", value: (c) => (isCloned(`${org}/${c.name}`) ? "cloned" : "—") }
    ])}\n`
  );
}

/**
 * Where a customer's repo is, and nothing else.
 *
 * One line on stdout so it composes: `cd "$(oxyc path pokehouse)"`. Everything
 * explanatory goes to stderr for exactly that reason.
 */
export function runPath(_ctx: Context, name: string, flags: { refresh?: boolean }): void {
  const customer = resolveCustomer(name, { refresh: flags.refresh });
  const slug = `${customersOrg()}/${customer.name}`;
  const path = dossierPath(slug);
  process.stdout.write(`${path}\n`);
  if (!isCloned(slug)) {
    process.stderr.write(
      `${out.yellow("not cloned here yet")} — ${out.dim(`gh repo clone ${slug} ${path}`)}\n`
    );
  }
}
