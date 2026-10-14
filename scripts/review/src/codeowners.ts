// CODEOWNERS parsing: who may speak for a path. Last matching rule wins, as on GitHub.

import { ownerPatternToRegExp } from "./globs.ts";

export interface OwnerRule {
  pattern: string;
  regex: RegExp;
  owners: string[];
}

export function parseCodeowners(text: string | null | undefined): OwnerRule[] {
  const rules: OwnerRule[] = [];
  for (const raw of String(text ?? "").split("\n")) {
    const line = raw.replace(/\s#.*$/, "").trim();
    if (!line || line.startsWith("#")) continue;
    const [pattern = "", ...owners] = line.split(/\s+/);
    rules.push({
      pattern,
      regex: ownerPatternToRegExp(pattern),
      owners: owners.filter((o) => o.startsWith("@"))
    });
  }
  return rules;
}

/** Owners of a path: user logins and team slugs, without the `@`. */
export function ownersFor(path: string, rules: readonly OwnerRule[]): string[] {
  let owners: string[] = [];
  for (const r of rules) if (r.regex.test(path)) owners = r.owners;
  return owners.map((o) => o.slice(1));
}
