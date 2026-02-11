/**
 * Encodes a string to base64, handling Unicode characters properly.
 * Unlike btoa(), this function supports non-Latin1 characters (e.g., emojis, CJK).
 */
export function encodeBase64(str: string): string {
  return btoa(
    encodeURIComponent(str).replace(/%([0-9A-F]{2})/g, (_, p1) =>
      String.fromCharCode(parseInt(p1, 16))
    )
  );
}

/**
 * Decodes a base64 string that was encoded with encodeBase64.
 */
export function decodeBase64(str: string): string {
  return decodeURIComponent(
    atob(str)
      .split("")
      .map((c) => `%${(`00${c.charCodeAt(0).toString(16)}`).slice(-2)}`)
      .join("")
  );
}
