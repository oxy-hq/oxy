/**
 * Utility functions for GitHub token operations
 */

/**
 * Validates the format of a GitHub personal access token
 * @param token - The token to validate
 * @returns boolean indicating if the token format is valid
 */
export const isValidTokenFormat = (token: string): boolean => {
  // GitHub personal access tokens start with 'ghp_' followed by 36 characters
  // For now, just check that it's not empty and has reasonable length
  return token.trim().length >= 20;
};

/**
 * Masks a GitHub token for display purposes
 * @param token - The token to mask
 * @returns Masked token string
 */
export const maskToken = (token: string): string => {
  if (token.length <= 8) return "*".repeat(token.length);

  const start = token.slice(0, 4);
  const end = token.slice(-4);
  const middle = "*".repeat(Math.max(token.length - 8, 4));

  return `${start}${middle}${end}`;
};

/**
 * Opens a URL in a new tab/window with security measures
 * @param url - The URL to open
 */
export const openSecureWindow = (url: string): void => {
  const newWindow = window.open(url, "_blank", "noopener,noreferrer");
  if (newWindow) newWindow.opener = null;
};
