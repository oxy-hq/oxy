import type { RecipeVariantProps } from "styled-system/css";

import { css, cva } from "styled-system/css";

import Icon from "../Icon";
import { SvgAssets } from "../Icon/Dictionary";

const containerStyles = css({
  display: "flex",
  justifyContent: "center",
  alignItems: "center",
  w: "md",
  h: "md",
});

const iconContainerStyles = css({
  display: "flex",
  w: "8px",
  h: "8px",
  color: "background.primary",
});

const statusIndicatorStyles = cva({
  base: {
    display: "flex",
    justifyContent: "center",
    alignItems: "center",
    borderRadius: "full",
  },
  variants: {
    status: {
      success: {
        backgroundColor: "token(colors.text.success)",
      },
      error: {
        backgroundColor: "token(colors.text.error)",
      },
      process: {
        backgroundColor: "token(colors.text.progress)",
      },
    },
    type: {
      default: {
        w: "6px",
        h: "6px",
      },
      icon: {
        w: "12px",
        h: "12px",
      },
    },
  },
  compoundVariants: [
    {
      status: "success",
      type: "default",
      css: {
        filter: "drop-shadow(0px 0px 4px rgba(30, 155, 112, 0.40))",
      },
    },
    {
      status: "error",
      type: "default",
      css: {
        filter: "drop-shadow(0px 0px 4px rgba(192, 68, 56, 0.40))",
      },
    },
    {
      status: "process",
      type: "default",
      css: {
        filter: "drop-shadow(0px 0px 4px rgba(237, 129, 50, 0.40))",
      },
    },
  ],
});

type StyleProps = NonNullable<RecipeVariantProps<typeof statusIndicatorStyles>>;

export const iconMap: Partial<
  Record<NonNullable<StyleProps["status"]>, SvgAssets>
> = {
  success: "check",
  error: "close",
};

function StatusIndicator({ status = "success", type = "default" }: StyleProps) {
  const iconAsset = iconMap[status];

  return (
    <div className={containerStyles}>
      <div className={statusIndicatorStyles({ status, type })}>
        {type === "icon" && iconAsset && (
          <span className={iconContainerStyles}>
            <Icon size="fromIcon" asset={iconAsset} />
          </span>
        )}
      </div>
    </div>
  );
}

export default StatusIndicator;
