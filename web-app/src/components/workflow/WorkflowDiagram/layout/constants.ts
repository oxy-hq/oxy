export const distanceBetweenNodes = 20;
export const distanceBetweenHeaderAndContent = 8;
export const contentPadding = 8;
export const contentPaddingHeight = contentPadding * 2;
export const nodePadding = 8;
export const paddingHeight = nodePadding * 2;
export const headerHeight = 40;
export const smallestNodeWidth = 200;
export const nodeBorder = 1;
export const nodeBorderHeight = nodeBorder * 2;
export const normalNodeHeight = headerHeight + paddingHeight + nodeBorderHeight;
export const minNodeWidth = 220;

/**
 * Vertical slot reserved below the loop-node header for the live
 * progress bar (`LoopProgressBar`). The bar's actual rendered
 * height (track + count line + spacing) must fit inside this slot.
 * Both `nodeSize.ts` (parent container height) and `elkLayout.ts`
 * (top padding ELK uses to offset children) consume this constant
 * so the bar and the children-row never collide.
 *
 * Sized for: 6px bar (`h-1.5`) + 4px gap + 10px count text = 20px,
 * plus 2px breathing room.
 */
export const loopProgressBarHeight = 22;
