import { OXY_MARK_PATH } from "./OxyMark";

/** The Oxygen "O" framed inside a hexagon — the mark for **Oxygen Factory**,
 *  the intelligence system powering the HQ. The structured frame reads as
 *  "system / substrate"; the brand O signals it's powered by Oxygen. A
 *  generic graph/gear icon couldn't carry that, so we compose the brand
 *  glyph rather than borrow one. Inherits `currentColor` like the other
 *  rail icons (dims when inactive, brightens when active). */
export function OxyCoreMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox='0 0 24 24'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={className}
      aria-hidden='true'
    >
      {/* Flat-top hexagon frame filling the viewBox, centered on (12,12). */}
      <path
        d='M23 12 L17.5 21.53 L6.5 21.53 L1 12 L6.5 2.47 L17.5 2.47 Z'
        stroke='currentColor'
        strokeWidth={1.5}
        strokeLinejoin='round'
      />
      {/* The Oxygen O, scaled to fill the frame so the mark reads as
          "O badge", not just a hexagon. */}
      <path
        d={OXY_MARK_PATH}
        fill='currentColor'
        fillRule='evenodd'
        clipRule='evenodd'
        transform='translate(2.76 2.76) scale(0.42)'
      />
    </svg>
  );
}
