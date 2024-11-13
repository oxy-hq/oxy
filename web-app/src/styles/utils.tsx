type TokenFn = (path: string) => string | undefined;

export const extendedUtils = {
  customScrollbar: {
    className: "customScrollbar",
    values: { type: "boolean" },
    transform(value: boolean, { token }: { token: TokenFn }) {
      if (!value) return {};
      return {
        WebkitOverflowScrolling: "touch",
        "&::-webkit-scrollbar": {
          // Custom width for scrollbar
          height: "6px",
          width: "6px",
          background: "transparent",
          borderRadius: token("radii.full")
        },
        "&::-webkit-scrollbar-thumb": {
          background: token("colors.border.primary"),
          borderRadius: token("radii.full")
        },
        "&::-webkit-scrollbar-track": {
          background: "transparent"
        }
      };
    }
  },
  rightSlideAnimation: {
    className: "rightSlideAnimation",
    values: { type: "boolean" },
    transform(value: boolean, { token }: { token: TokenFn }) {
      if (!value) return {};
      return {
        "--skeleton-loader-color-stop-1": token("colors.surface.secondary"),
        "--skeleton-loader-color-stop-2": token("colors.surface.tertiary"),
        "--animation-duration": "1.5s",
        "--animation-direction": "normal",
        "--pseudo-element-display": "block",
        background: "var(--skeleton-loader-color-stop-1)",

        borderRadius: token("radii.minimal"),
        display: "inline-flex",
        lineHeight: "1",
        position: "relative",
        userSelect: "none",
        overflow: "hidden",
        zIndex:
          "1" /* Necessary for overflow: hidden to work correctly in Safari */,
        "&::after": {
          content: "' '",
          display: "var(--pseudo-element-display)",
          position: "absolute",
          top: "0",
          left: "0",
          right: "0",
          height: "100%",
          backgroundRepeat: "no-repeat",
          background:
            "linear-gradient(90deg, var(--skeleton-loader-color-stop-1), var(--skeleton-loader-color-stop-2), var(--skeleton-loader-color-stop-1))",
          transform: "translateX(-100%)",
          animationName: "slideRight",
          animationDuration: "var(--animation-duration)",
          animationDirection: "var(--animation-direction)",
          animationTimingFunction: "ease-in-out",
          animationIterationCount: "infinite"
        }
      };
    }
  }
};
