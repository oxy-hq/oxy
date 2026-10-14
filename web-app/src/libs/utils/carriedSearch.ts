/**
 * The `search` a redirect keeps from the URL it leaves: only the named params,
 * and only those with a value. The post-login and org dispatchers use it to carry
 * deep links (`?settings=<section>`, `?notifications=open`) into the workspace they
 * pick, without forwarding the rest of the query.
 */
export function carriedSearch(from: URLSearchParams, names: readonly string[]): string {
  const kept = new URLSearchParams();
  for (const name of names) {
    const value = from.get(name);
    if (value) kept.set(name, value);
  }
  const search = kept.toString();
  return search ? `?${search}` : "";
}
