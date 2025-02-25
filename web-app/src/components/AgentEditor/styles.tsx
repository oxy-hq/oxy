import { css } from "styled-system/css";
import { hstack, stack, vstack } from "styled-system/patterns";

export const listItemStyle = hstack({
  cursor: "pointer",
  flex: 1,
  borderRadius: "rounded",
  boxShadow:
    "inset 0 0 0 1px token(colors.neutral.border.colorBorderSecondary)",
  p: "sm",
  gap: "sm",
  _hover: {
    bg: "neutral.bg.colorBgHover",
    boxShadow: "unset",
  },
});

export const modalHeaderStyles = css({
  w: "100%",
  p: "xl",
  position: "relative",
  boxShadow: "inset 0 -1px 0 0 token(colors.border.primary)",
});

export const formWrapperStyles = stack({
  gap: "xl",
  alignItems: "stretch",
  h: "full",
  minH: 0,
});

export const formContentStyles = vstack({
  gap: "4xl",
  alignItems: "stretch",
  customScrollbar: true,
  flex: 1,
  overflow: "auto",
  minH: 0,
  p: "xl",
});

export const modalContentStyles = css({
  w: "400px",
  maxH: "100%",
  display: "flex",
  flexDirection: "column",
});
