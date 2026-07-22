import * as React from "react";

/**
 * Floating shell content (tooltips) portals into the nearest shell scope
 * element instead of `document.body`, so the `oxy-shell-scope` tokens and a
 * host-applied `.dark` ancestor keep applying to it. Falls back to the
 * default body portal when no provider is mounted (standalone `ShellRail`
 * under a host that themes `<body>` itself).
 */
export const ShellPortalContext = React.createContext<HTMLElement | null>(null);

/** Track the portal container element for descendants of a shell root. */
export function useShellPortalContainer(): {
  container: HTMLElement | null;
  setContainer: (el: HTMLElement | null) => void;
} {
  const [container, setContainer] = React.useState<HTMLElement | null>(null);
  return { container, setContainer };
}
