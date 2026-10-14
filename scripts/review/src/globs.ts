// Glob matching for config path lists and CODEOWNERS, without a dependency.

const escape = (s: string): string => s.replace(/[.+^$()|[\]\\]/g, "\\$&");

/** Translate the body of a glob (no anchoring) into a regex source. */
function body(glob: string): string {
  let out = "";
  for (let i = 0; i < glob.length; i++) {
    const c = glob.charAt(i);
    if (c === "*" && glob[i + 1] === "*") {
      const slash = glob[i + 2] === "/";
      out += slash ? "(?:.*/)?" : ".*";
      i += slash ? 2 : 1;
    } else if (c === "*") {
      out += "[^/]*";
    } else if (c === "?") {
      out += "[^/]";
    } else if (c === "{") {
      const end = glob.indexOf("}", i);
      if (end === -1) {
        out += "\\{";
        continue;
      }
      out += `(?:${glob
        .slice(i + 1, end)
        .split(",")
        .map(escape)
        .join("|")})`;
      i = end;
    } else {
      out += escape(c);
    }
  }
  return out;
}

/** A config glob: matched against the whole repo-relative path. */
export function globToRegExp(glob: string): RegExp {
  return new RegExp(`^${body(glob)}$`);
}

/**
 * A CODEOWNERS / gitignore pattern: a leading `/` or an inner `/` anchors it at the
 * root, otherwise it matches at any depth; a match on a directory covers what is in it.
 */
export function ownerPatternToRegExp(pattern: string): RegExp {
  let p = pattern.trim();
  const dir = p.endsWith("/");
  if (dir) p = p.slice(0, -1);
  const anchored = p.startsWith("/") || p.includes("/");
  if (p.startsWith("/")) p = p.slice(1);
  const prefix = anchored ? "" : "(?:.*/)?";
  const tail = dir ? "/.*" : "(?:/.*)?";
  return new RegExp(`^${prefix}${body(p)}${tail}$`);
}

export function matchesAny(path: string, globs: readonly string[]): boolean {
  return globs.some((g) => globToRegExp(g).test(path));
}
