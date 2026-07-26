/**
 * Radix Dialog / Popover / Sheet set `pointer-events: none` on `<body>` while
 * open and clear it on close. When a component closes *and* navigates in the
 * same tick, that cleanup can miss and the lock leaks onto the destination
 * page — leaving it (and any control on it, like the org switcher) unclickable.
 *
 * Call this at those transition points, and on the destination's mount, to
 * force-clear the lock. Safe to call unconditionally: if nothing set the lock,
 * removing the property is a no-op.
 */
export function releaseBodyPointerLock(): void {
  document.body.style.removeProperty("pointer-events");
}
