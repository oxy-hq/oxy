import { ForwardedRef, forwardRef } from "react";

import type { RecipeVariantProps } from "styled-system/css";

import invariant from "invariant";
import { cva, cx } from "styled-system/css";

import { SVG_DICTIONARY, SvgAssets } from "./Dictionary";

export type IconProps = {
  asset: SvgAssets;
  className?: string;
};

const SMALL_ICON_SIZE = 12;
const DEFAULT_ICON_SIZE = 20;

const iconStyles = cva({
  base: {
    display: "inline-flex",
    alignItems: "center",
    color: "inherit",
    flexShrink: "0",
  },
  variants: {
    size: {
      small: {
        width: `${SMALL_ICON_SIZE}px`,
        height: `${SMALL_ICON_SIZE}px`,
      },
      default: {
        width: `${DEFAULT_ICON_SIZE}px`,
        height: `${DEFAULT_ICON_SIZE}px`,
      },
      fromIcon: {
        width: "100%",
        height: "100%",
      },
    },
  },
});

export type ButtonProps = IconProps & RecipeVariantProps<typeof iconStyles>;

const Icon = forwardRef(function IconWithRef(
  { size = "default", asset, className, ...props }: ButtonProps,
  iconRef: ForwardedRef<HTMLDivElement>,
) {
  if (!SVG_DICTIONARY[asset]) {
    invariant(false, `Icon ${asset} does not exists`);
  }

  const { path, width, height, viewBox } = SVG_DICTIONARY[asset];

  return (
    <div
      className={cx(iconStyles({ size }), className)}
      ref={iconRef}
      {...props}
    >
      <svg
        width={width ?? "100%"}
        height={height ?? "100%"}
        version="1.1"
        viewBox={viewBox ?? "0 0 20 20"}
        fill="none"
      >
        {path}
      </svg>
    </div>
  );
});

export default Icon;
