/**
 * Resolve a CSS variable name (e.g. "--primary") to an actual color value
 * that ECharts canvas rendering can use.
 *
 * ECharts renders on <canvas> and cannot interpret CSS functions like
 * `var(...)` or `color-mix(...)`. This helper applies the variable to a
 * temporary DOM element and reads back the computed `rgb(...)` value.
 */
export const resolveColor = (cssVarName: string): string => {
  const raw = getComputedStyle(document.body).getPropertyValue(cssVarName).trim();
  const match = raw.match(/[\d.]+/g);
  if (!match || match.length < 3) return raw;
  const [r, g, b] = match.map(Number);
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
};

/**
 * Resolve a CSS variable with an alpha modifier.
 * Returns an `rgba(...)` string suitable for ECharts.
 */
export const resolveColorWithAlpha = (cssVarName: string, alpha: number): string => {
  const hex = resolveColor(cssVarName);
  const match = hex.match(/^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
  if (!match) return hex;
  const [, r, g, b] = match.map((v, i) => (i === 0 ? v : parseInt(v, 16)));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
};
