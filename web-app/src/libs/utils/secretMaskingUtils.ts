/**
 * Masks a secret value by showing only the first and last few characters
 * @param secret The secret string to mask
 * @param visibleStart Number of characters to show at the start (default: 3)
 * @param visibleEnd Number of characters to show at the end (default: 3)
 * @param maskChar Character to use for masking (default: '*')
 * @returns Masked secret string
 */
export function maskSecret(
  secret: string,
  visibleStart: number = 3,
  visibleEnd: number = 3,
  maskChar: string = "*",
): string {
  if (!secret || typeof secret !== "string") {
    return "";
  }

  // If secret is too short, mask most of it but show at least one character
  if (secret.length <= 4) {
    return secret.charAt(0) + maskChar.repeat(Math.max(secret.length - 1, 3));
  }

  // If secret is shorter than visible chars, reduce the visible chars
  const actualVisibleStart = Math.min(
    visibleStart,
    Math.floor(secret.length / 3),
  );
  const actualVisibleEnd = Math.min(visibleEnd, Math.floor(secret.length / 3));

  const start = secret.substring(0, actualVisibleStart);
  const end = secret.substring(secret.length - actualVisibleEnd);
  const middleLength = Math.max(
    secret.length - actualVisibleStart - actualVisibleEnd,
    6,
  );

  return start + maskChar.repeat(middleLength) + end;
}

/**
 * Masks a secret value specifically for display in tables or lists
 * Shows fewer characters for compact display
 * @param secret The secret string to mask
 * @returns Masked secret string optimized for table display
 */
export function maskSecretForTable(secret: string): string {
  return maskSecret(secret, 2, 2, "•");
}

/**
 * Completely masks a secret showing only asterisks
 * @param secret The secret string to mask
 * @param length Fixed length of the mask (default: 8)
 * @returns Completely masked string
 */
export function maskSecretCompletely(
  secret: string,
  length: number = 8,
): string {
  if (!secret || typeof secret !== "string") {
    return "";
  }

  return "*".repeat(length);
}

/**
 * Validates if a string is likely to be a secret (contains sensitive patterns)
 * @param value The string to check
 * @returns true if the string appears to be a secret
 */
export function isLikelySecret(value: string): boolean {
  if (!value || typeof value !== "string") {
    return false;
  }

  const secretPatterns = [
    /^sk-[a-zA-Z0-9]{48,}$/, // OpenAI API keys
    /^xoxb-[a-zA-Z0-9-]+$/, // Slack bot tokens
    /^ghp_[a-zA-Z0-9]{36}$/, // GitHub personal access tokens
    /^ghs_[a-zA-Z0-9]{36}$/, // GitHub app tokens
    /^[A-Za-z0-9+/]{40,}={0,2}$/, // Base64 encoded (likely token)
    /^[a-f0-9]{32,}$/i, // Hex strings (MD5, SHA, etc.)
    /password|secret|key|token/i, // Contains sensitive keywords
  ];

  return (
    secretPatterns.some((pattern) => pattern.test(value)) ||
    (value.length >= 20 && !/\s/.test(value))
  ); // Long strings without spaces
}

/**
 * Validates a secret name according to naming conventions
 * @param name The secret name to validate
 * @returns Object with validation result and error message
 */
export function validateSecretName(name: string): {
  isValid: boolean;
  error?: string;
} {
  if (!name || typeof name !== "string") {
    return { isValid: false, error: "Secret name is required" };
  }

  const trimmedName = name.trim();

  if (trimmedName.length === 0) {
    return { isValid: false, error: "Secret name cannot be empty" };
  }

  if (trimmedName.length > 100) {
    return {
      isValid: false,
      error: "Secret name cannot exceed 100 characters",
    };
  }

  if (!/^[a-zA-Z0-9_-]+$/.test(trimmedName)) {
    return {
      isValid: false,
      error:
        "Secret name can only contain letters, numbers, hyphens, and underscores",
    };
  }

  if (
    trimmedName.startsWith("-") ||
    trimmedName.endsWith("-") ||
    trimmedName.startsWith("_") ||
    trimmedName.endsWith("_")
  ) {
    return {
      isValid: false,
      error: "Secret name cannot start or end with hyphens or underscores",
    };
  }

  return { isValid: true };
}
