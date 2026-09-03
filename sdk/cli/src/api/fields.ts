/**
 * `-f` and `-F`, with `gh api`'s semantics.
 *
 * The letters and their meanings are gh's, not ours, because the whole point
 * of this command is that muscle memory transfers:
 *
 *   -f, --raw-field key=value   ALWAYS a string
 *   -F, --field     key=value   typed — true/false/null/123 keep their JSON
 *                               type, `@file` reads a file, `@-` reads stdin,
 *                               and anything else stays a string
 *
 * The fallback-to-string in `-F` is what makes a bare UUID work: it is not
 * valid JSON, and erroring on it would make the typed flag unusable for the
 * single most common value in this API.
 */

import { readFileSync } from "node:fs";

import { usageError } from "../util/errors.js";

export type FieldValue = string | number | boolean | null | FieldValue[];

/** Split `key=value`, naming the flag in the error so it says which one. */
function splitField(flag: string, raw: string): [string, string] {
  const idx = raw.indexOf("=");
  if (idx < 0) {
    throw usageError(`invalid ${flag} '${raw}', expected key=value`);
  }
  return [raw.slice(0, idx), raw.slice(idx + 1)];
}

/** Everything on stdin, as a string. Read at most once per process. */
let stdinCache: string | undefined;
function readStdin(): string {
  if (stdinCache === undefined) {
    try {
      stdinCache = readFileSync(0, "utf8");
    } catch {
      stdinCache = "";
    }
  }
  return stdinCache;
}

/**
 * A `-F` value, typed.
 *
 * `@` prefixes are checked before JSON so a value that is *both* — `@1` — is
 * read as a filename, which is gh's behaviour and the less surprising of the
 * two: nobody writes `@1` meaning the number 1.
 */
export function parseTypedValue(raw: string): FieldValue {
  if (raw === "@-") return readStdin().trim();
  if (raw.startsWith("@")) {
    const path = raw.slice(1);
    try {
      return readFileSync(path, "utf8").trim();
    } catch (cause) {
      throw usageError(`could not read ${path}: ${(cause as Error).message}`);
    }
  }
  if (raw === "true") return true;
  if (raw === "false") return false;
  if (raw === "null") return null;
  try {
    // JSON.parse handles numbers, arrays and objects. A bare word throws,
    // which is the fallback-to-string case below.
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== "object") return parsed as FieldValue;
    return parsed as FieldValue;
  } catch {
    return raw;
  }
}

export interface ParsedFields {
  /** The assembled parameter object, ready to be a body or a query string. */
  params: Record<string, FieldValue>;
  /** Whether any field was given at all — distinct from "an empty object". */
  present: boolean;
}

/**
 * Assemble `-f` and `-F` into one object.
 *
 * Typed fields are applied second so a key given to both wins as the typed
 * value — the more specific instruction wins, and the alternative (first one
 * wins) would make `-f x=1 -F x=1` depend on argv order.
 *
 * A key ending in `[]` accumulates into an array across repeats, so
 * `-F 'ids[]=a' -F 'ids[]=b'` sends `{"ids":["a","b"]}`. Without it, sending a
 * two-element array means hand-writing JSON into a single flag, which is
 * exactly the escaping misery `-F` exists to remove.
 */
export function parseFields(rawFields: string[], typedFields: string[]): ParsedFields {
  const params: Record<string, FieldValue> = {};

  const assign = (key: string, value: FieldValue) => {
    if (key.endsWith("[]")) {
      const name = key.slice(0, -2);
      const existing = params[name];
      params[name] = Array.isArray(existing) ? [...existing, value] : [value];
      return;
    }
    params[key] = value;
  };

  for (const raw of rawFields) {
    const [key, value] = splitField("--raw-field", raw);
    assign(key, value);
  }
  for (const raw of typedFields) {
    const [key, value] = splitField("--field", raw);
    assign(key, parseTypedValue(value));
  }

  return { params, present: rawFields.length > 0 || typedFields.length > 0 };
}

/**
 * Params as a query string, for a method that carries no body.
 *
 * `gh api` moves fields to the query on GET/HEAD/DELETE rather than refusing
 * them, and this API has plenty of GET endpoints with parameters
 * (`?page=`, `?limit=`, `?branch=`), so the alternative would be forcing
 * callers back to hand-built URLs for the commonest case.
 *
 * An array becomes repeated keys (`ids=a&ids=b`) — what axum's
 * `Query<Vec<_>>` extractors read — and `null` is dropped rather than sent as
 * the four letters "null", which no query parser reads as absence.
 */
export function paramsToQuery(params: Record<string, FieldValue>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined) continue;
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item !== null) search.append(key, String(item));
      }
      continue;
    }
    search.append(key, String(value));
  }
  return search.toString();
}
