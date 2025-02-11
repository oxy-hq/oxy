import { css, cva } from "styled-system/css";

export const DEFAULT_SIDEBAR_WIDTH = 330;

const dynamicSidebarWidthHandler = css({
  display: {
    base: "none",
    sm: "block",
  },
  width: {
    sm: "60px",
    xl: "244px",
  },
});

const desktopSidebarWrapperStyles = css({
  display: {
    base: "none",
    xl: "block",
  },
  bg: "background.primary",
});

const mobileSidebarWrapperStyles = css({
  display: {
    base: "block",
    xl: "none",
  },
  bg: "background.primary",
});

const collapsedMobileSidebarStyles = css({
  padding: "md",
  display: {
    base: "none",
    sm: "flex",
  },
  flexDirection: "column",
  height: "100%",
  gap: "sm",
});

const sidebarMobileBackdropStyles = cva({
  base: {
    display: "block",
    position: "fixed",
    top: "0",
    bottom: "0",
    left: "0",
    right: "0",
    width: "100%",
    height: "100%",
    bg: "background.opacity",
    zIndex: "99",
  },
  variants: {
    state: {
      open: {
        base: {
          visibility: "visible",
          opacity: "1",
          transition: "opacity 0.2s ease-in-out",
        },
        sm: {},
      },
      closed: {
        base: {
          visibility: "hidden",
          opacity: "0",
          transition: "opacity 0.2s ease-in-out",
        },
        sm: {},
      },
    },
  },
});

const mobileSidebarStyles = cva({
  base: {
    position: "fixed",
    top: "0",
    bottom: "0",
    left: "0",
    zIndex: "100",
    bg: "background.primary",
    transition: "transform 0.2s ease-in-out",
    w: {
      base: "80%",
      sm: "230px",
    },
  },
  variants: {
    state: {
      open: {
        transform: "translateX(0)",
      },
      closed: {
        transform: "translateX(-100%)",
      },
    },
  },
});

const sidebarStyles = css({
  position: "fixed",
  top: "0",
  bottom: "0",
  left: "0",
  zIndex: "100",
});

const sidebarCollapsedStyles = cva({
  base: {
    padding: "md",
    display: "flex",
    flexDirection: "column",
    height: "100%",
    gap: "sm",
  },
  variants: {
    state: {
      open: {
        display: "none",
      },
      closed: {
        display: "flex",
      },
    },
  },
});

const sidebarInnerStyles = css({
  position: "relative",
  display: "flex",
  flexDirection: "column",
  flexGrow: 0,
  height: "100%",
  justifyContent: "space-between",
  gap: "xs",
});

const sidebarNavigationItems = css({
  display: "flex",
  flexDirection: "column",
  gap: "xs",
  overflowY: "auto",
  overflowX: "hidden",
  flexGrow: 1,
  pb: "2px",
  customScrollbar: true,
  px: "lg",
});

const sidebarResizer = cva({
  base: {
    position: "absolute",
    right: "-4.5px",
    height: "100%",
    width: "7px",
    cursor: "col-resize",
    bg: "transparent",
    display: {
      base: "none",
      xl: "block",
    },
  },
  variants: {
    variant: {
      "no-header": {
        top: "0",
        py: "2xl",
      },
      "with-header": {
        py: "48px",
        top: "22px",
      },
    },
  },
});

const sidebarHandler = css({
  position: "relative",
  height: "100%",
  _before: {
    content: "' '",
    position: "absolute",
    top: "0",
    right: "3.5px",
    height: "100%",
    width: "1px",
  },
  _hover: {
    _before: {
      bg: "border.light",
    },
  },
});

const profileSkeletonStyles = css({
  display: "flex",
  flex: 1,
  gap: "sm",
  alignItems: "center",
});

const bottomActionsStyles = css({
  display: "flex",
  flexDirection: "column",
  gap: "xs",
});

const mainSidebarContentStyles = css({
  display: "flex",
  alignItems: "center",
  px: "lg",
  h: "56px",
});

const sidebarHeadStyles = css({
  display: "flex",
  flexDirection: "column",
  gap: "xxs",
});

export {
  dynamicSidebarWidthHandler,
  sidebarMobileBackdropStyles,
  sidebarCollapsedStyles,
  sidebarStyles,
  sidebarInnerStyles,
  sidebarNavigationItems,
  sidebarResizer,
  sidebarHandler,
  profileSkeletonStyles,
  bottomActionsStyles,
  mainSidebarContentStyles,
  mobileSidebarStyles,
  desktopSidebarWrapperStyles,
  mobileSidebarWrapperStyles,
  collapsedMobileSidebarStyles,
  sidebarHeadStyles,
};
