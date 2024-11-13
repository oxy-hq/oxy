import React from "react";
import { cva, cx, RecipeVariantProps } from "styled-system/css";
import { Slot } from "@radix-ui/react-slot";

const codeStyles = cva({
  variants: {
    variant: {
      code14Regular: {
        textStyle: "code14Regular"
      }
    }
  }
});

export type CodeProps = {
  as?: "div" | "span" | "code";
  asChild?: boolean;
  className?: string;
  children?: React.ReactNode;
} & RecipeVariantProps<typeof codeStyles>;

type CodeElement = React.ElementRef<"code">;

const Code = React.forwardRef<CodeElement, CodeProps>((props, forwardedRef) => {
  const {
    children,
    className,
    variant = "code14Regular",
    asChild = false,
    as: Tag = "span",
    ...headingProps
  } = props;
  return (
    <Slot
      {...headingProps}
      ref={forwardedRef}
      className={cx(codeStyles({ variant }), className)}
    >
      {asChild ? children : <Tag>{children}</Tag>}
    </Slot>
  );
});

Code.displayName = "Code";

export default Code;
