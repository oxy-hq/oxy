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
      error: "Secret name cannot exceed 100 characters"
    };
  }

  if (!/^[a-zA-Z0-9_-]+$/.test(trimmedName)) {
    return {
      isValid: false,
      error: "Secret name can only contain letters, numbers, hyphens, and underscores"
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
      error: "Secret name cannot start or end with hyphens or underscores"
    };
  }

  return { isValid: true };
}
