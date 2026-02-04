/**
 * Utility functions for cleaning form data by removing empty/meaningless values
 */

export const isMeaningfulValue = (value: unknown): boolean => {
  if (value === undefined || value === null || value === "") {
    return false;
  }
  if (typeof value === "number" && Number.isNaN(value)) {
    return false;
  }
  if (typeof value === "string" && !value.trim()) {
    return false;
  }
  return true;
};

export const cleanObject = (obj: Record<string, unknown>): Record<string, unknown> | null => {
  const cleaned: Record<string, unknown> = {};

  Object.entries(obj).forEach(([key, value]) => {
    // Handle arrays
    if (Array.isArray(value)) {
      const cleanedArray = value
        .map((item) => {
          if (item && typeof item === "object") {
            return cleanObject(item as Record<string, unknown>);
          }
          return isMeaningfulValue(item) ? item : null;
        })
        .filter((item) => item !== null);

      if (cleanedArray.length > 0) {
        cleaned[key] = cleanedArray;
      }
    }
    // Handle nested objects recursively
    else if (value && typeof value === "object") {
      const cleanedNested = cleanObject(value as Record<string, unknown>);
      if (cleanedNested) {
        cleaned[key] = cleanedNested;
      }
    }
    // Handle primitive values
    else if (isMeaningfulValue(value)) {
      cleaned[key] = value;
    }
  });

  return Object.keys(cleaned).length > 0 ? cleaned : null;
};
