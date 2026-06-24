import type { FieldItem } from "./types";

/**
 * Returns the items list with the current value added if it doesn't exist.
 * This handles the case where a value was previously set but is no longer
 * in the available options (e.g., topic/field was renamed or deleted).
 */
export const getItemsWithUnknownValue = (
  items: FieldItem[],
  value: string | undefined
): FieldItem[] => {
  if (!value) {
    return items;
  }

  if (items.some((item) => item.value === value)) {
    return items;
  }

  // Add the unknown value at the beginning of the list
  return [
    {
      value,
      label: value,
      searchText: value.toLowerCase()
    },
    ...items
  ];
};
