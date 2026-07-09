import { useEffect, useState } from "react";

/**
 * Subscribe to a CSS media query and get whether it currently matches, updating
 * on viewport changes (resize, orientation, zoom). Initializes from
 * `matchMedia` on first render so there's no post-mount flash. Client-only app,
 * but guards `window` so it never throws in a non-DOM context.
 *
 * @example const isWide = useMediaQuery("(min-width: 1024px)");
 */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window === "undefined" ? false : window.matchMedia(query).matches
  );

  useEffect(() => {
    const mql = window.matchMedia(query);
    const onChange = () => setMatches(mql.matches);
    onChange();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [query]);

  return matches;
}
