import React from "react";

import { Slot } from "@radix-ui/react-slot";
import { cva, cx, RecipeVariantProps } from "styled-system/css";

const headingStyles = cva({
  variants: {
    variant: {
      headline20Medium: {
        textStyle: "headline20Medium",
      },
    },
  },
});

export type HeadingProps = {
  as?: "div" | "span" | "h1" | "h2" | "h3" | "h4";
  asChild?: boolean;
  children?: React.ReactNode;
  className?: string;
} & RecipeVariantProps<typeof headingStyles>;

type HeadingElement = React.ElementRef<"h1">;

const Heading = React.forwardRef<HeadingElement, HeadingProps>(
  (props, forwardedRef) => {
    const {
      children,
      className,
      asChild = false,
      as: Tag = "span",
      variant = "headline20Medium",
      ...headingProps
    } = props;
    return (
      <Slot
        {...headingProps}
        ref={forwardedRef}
        className={cx(headingStyles({ variant }), className)}
      >
        {asChild ? children : <Tag>{children}</Tag>}
      </Slot>
    );
  },
);

Heading.displayName = "Text";

export default Heading;
