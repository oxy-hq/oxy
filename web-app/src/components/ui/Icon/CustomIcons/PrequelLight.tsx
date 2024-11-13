import { forwardRef } from "react";
import { css } from "styled-system/css";

export interface IconProps extends React.SVGAttributes<SVGElement> {
  children?: never;
  color?: string;
}

export const PrequelLight = forwardRef<SVGSVGElement, IconProps>(
  (props, forwardedRef) => {
    return (
      <svg
        width="160"
        height="188"
        viewBox="0 0 160 188"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        ref={forwardedRef}
        {...props}
      >
        <g filter="url(#filter0_i_105_4631)">
          <path
            className={css({
              fill: "surface.secondary"
            })}
            fillRule="evenodd"
            clipRule="evenodd"
            d="M80 188L160 140.867V47.1333L80 0L0 47.1333L0 140.867L80 188ZM13.3333 133.297V54.7029L45.9842 35.4661L66.6597 47.474L32.381 66.9473V144.519L13.3333 133.297ZM80 15.4252L59.1203 27.7268L83.3315 41.7882L127.619 66.9473V105.75L146.667 94.8927V54.7029L80 15.4252ZM146.667 133.297V110.189L124.265 122.958L80 148.104L45.7143 128.627V152.375L80 172.575L146.667 133.297ZM73.3944 129.068V99.5106L45.7143 82.7064V113.343L73.3944 129.068ZM52.0951 71.0319L80 55.1795L108.358 71.2895L80.1025 88.0349L52.0951 71.0319ZM114.286 83.2255L86.7277 99.5573V128.999L114.286 113.343V83.2255Z"
          />
        </g>
        <defs>
          <filter
            id="filter0_i_105_4631"
            x="0"
            y="0"
            width="160"
            height="190"
            filterUnits="userSpaceOnUse"
            colorInterpolationFilters="sRGB"
          >
            <feFlood floodOpacity="0" result="BackgroundImageFix" />
            <feBlend
              mode="normal"
              in="SourceGraphic"
              in2="BackgroundImageFix"
              result="shape"
            />
            <feColorMatrix
              in="SourceAlpha"
              type="matrix"
              values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0"
              result="hardAlpha"
            />
            <feOffset dy="2" />
            <feGaussianBlur stdDeviation="6" />
            <feComposite in2="hardAlpha" operator="arithmetic" k2="-1" k3="1" />
            <feColorMatrix
              type="matrix"
              values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0.08 0"
            />
            <feBlend
              mode="normal"
              in2="shape"
              result="effect1_innerShadow_105_4631"
            />
          </filter>
        </defs>
      </svg>
    );
  }
);

PrequelLight.displayName = "PrequelLight";
