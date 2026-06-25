/** Branded Ask prompt — teaches that Oxygen is the universal interface.
 *  Shared by the Ask dock composer, the Chat page, and the no-apps
 *  launcher fallback so the placeholder reads identically everywhere. */
export const askPlaceholder = (orgName?: string) =>
  `Ask Oxygen anything about ${orgName?.trim() || "your business"}…`;
