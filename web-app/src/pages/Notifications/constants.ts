/**
 * Wide enough for the category tree beside the list. Narrower — a phone, or a
 * small tablet in portrait — the list fills the screen and the tree is a filter
 * sheet. Tailwind's `md`, written as a `min-width` so the wide branch is the one
 * the test setup's always-matching `matchMedia` picks.
 */
export const WIDE_MEDIA_QUERY = "(min-width: 768px)";
