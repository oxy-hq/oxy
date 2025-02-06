import React, { useContext, useMemo } from "react";

import TextAreaAutosize from "react-textarea-autosize";
import { css, cva, cx } from "styled-system/css";

import {
  TextAreaContext,
  TextAreaElement,
  TextAreaProps,
  TextAreaRootElement,
  TextAreaRootProps,
  TextAreaSlotElement,
  TextAreaSlotProps,
} from "./types";

const textAreaStyles = cva({
  base: {
    display: "block",
    padding: 0,
    width: "100%",
    outline: "none",
    appearance: "none",
    fontFamily: "inherit",
    position: "relative",
    zIndex: 1,
    backgroundColor: "surface.primary",
    resize: "none",
    overflow: "hidden",
    height: {
      base: "38px",
      sm: "36px",
    },
  },
  variants: {
    variant: {
      default: {
        color: "text.primary",
        textStyle: {
          base: "paragraph16Regular",
          sm: "paragraph14Regular",
        },
        _placeholder: {
          color: "text.secondary",
        },
      },
    },
  },
});

const textAreaRootStyles = cva({
  base: {
    display: "flex",
    alignItems: "end",
    zIndex: 0,
    cursor: "text",
    backgroundColor: "surface.primary",
    w: "100%",
    pl: "xs",
  },
  variants: {
    variant: {
      default: {
        gap: "sm",
        borderRadius: "full",
      },
    },
  },
});

const textAreaSlotStyles = css({
  zIndex: 1,
});

const TextAreaRoot = React.forwardRef<TextAreaRootElement, TextAreaRootProps>(
  function TextAreaRoot(props, forwardedRef) {
    const { children, className, variant, ...rootProps } = props;
    const value = useMemo(() => {
      return { variant: variant };
    }, [variant]);

    return (
      <div
        ref={forwardedRef}
        {...rootProps}
        className={cx(textAreaRootStyles({ variant }), className)}
      >
        <TextAreaContext.Provider value={value}>
          {children}
        </TextAreaContext.Provider>
      </div>
    );
  },
);

const TextArea = React.forwardRef<TextAreaElement, TextAreaProps>(
  function TextArea(props, forwardedRef) {
    const context = useContext(TextAreaContext);
    const hasRoot = context !== undefined;

    const { className, variant = "default", ...rest } = props;
    const textArea = (
      <TextAreaAutosize
        ref={forwardedRef}
        // Need to set height for loading the component in Next.js due to
        // the following issue https://github.com/Andarist/react-textarea-autosize/issues/275
        // style={{
        //   height: 38 // 'sizes.4xl'
        // }}
        className={cx(textAreaStyles({ variant }), className)}
        maxRows={6}
        {...rest}
      />
    );

    return hasRoot ? (
      textArea
    ) : (
      <TextAreaRoot variant={variant}>{textArea}</TextAreaRoot>
    );
  },
);

const TextAreaSlot = React.forwardRef<TextAreaSlotElement, TextAreaSlotProps>(
  function TextAreaSlot(props, forwardedRef) {
    const { className, ...slotProps } = props;

    return (
      <div
        ref={forwardedRef}
        {...slotProps}
        className={cx(textAreaSlotStyles, className)}
      />
    );
  },
);

export { TextArea, TextAreaSlot, TextAreaRoot };
