import * as React from "react";

import * as ToastPrimitives from "@radix-ui/react-toast";
import { css, cx } from "styled-system/css";

const ToastProvider = ToastPrimitives.Provider;
const ToastAction = ToastPrimitives.Action;
const ToastClose = ToastPrimitives.Close;

const toastViewportStyles = css({
  display: "flex",
  position: "fixed",
  flexDirection: "column",
  zIndex: 104,
  gap: "sm",
  p: "2xl",
  top: "auto",
  bottom: 0,
  right: 0,
  maxHeight: "100vh"
});

const ToastViewport = React.forwardRef<
  React.ElementRef<typeof ToastPrimitives.Viewport>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitives.Viewport>
>(({ className, ...props }, ref) => (
  <ToastPrimitives.Viewport ref={ref} className={cx(toastViewportStyles, className)} {...props} />
));
ToastViewport.displayName = ToastPrimitives.Viewport.displayName;

const toastStyles = css({
  pointerEvents: "auto",
  pos: "relative",
  flexDirection: "row",
  justifyContent: "space-between",
  borderRadius: "rounded",
  bg: "surface.contrast",
  p: "lg",
  shadow: "secondary",
  transitionProperty: "all",
  transitionTimingFunction: "cubic-bezier(0.4, 0, 0.2, 1)",
  transitionDuration: "300ms",
  "&[data-swipe=move]": {
    transform: "translateX(var(--radix-toast-swipe-move-x))",
    transition: "none"
  },
  "&[data-swipe=cancel]": { transform: "translateX(0)" },
  "&[data-swipe=end]": {
    animation: "toastHide 100ms ease-in"
  },
  "&[data-state=closed]": {
    animation: "toastHide 200ms ease-in"
  },
  "&[data-state=open]": {
    animation: "toastShow 200ms ease-out"
  }
});

const Toast = React.forwardRef<
  React.ElementRef<typeof ToastPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitives.Root>
>(({ className, ...props }, ref) => {
  return (
    <ToastPrimitives.Root
      ref={ref}
      className={cx(toastStyles, "group", className)}
      onSwipeMove={(event) => {
        event.preventDefault();
      }}
      onSwipeEnd={(event) => {
        event.preventDefault();
      }}
      {...props}
    />
  );
});
Toast.displayName = ToastPrimitives.Root.displayName;

const titleStyles = css({
  color: "text.contrast",
  textStyle: "label14Regular"
});

const ToastTitle = React.forwardRef<
  React.ElementRef<typeof ToastPrimitives.Title>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitives.Title>
>(({ className, ...props }, ref) => (
  <ToastPrimitives.Title ref={ref} className={cx(titleStyles, className)} {...props} />
));
ToastTitle.displayName = ToastPrimitives.Title.displayName;

const descriptionStyles = css({
  color: "text.less-contrast",
  textStyle: "label14Regular"
});

const ToastDescription = React.forwardRef<
  React.ElementRef<typeof ToastPrimitives.Description>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitives.Description>
>(({ className, ...props }, ref) => (
  <ToastPrimitives.Description ref={ref} className={cx(descriptionStyles, className)} {...props} />
));
ToastDescription.displayName = ToastPrimitives.Description.displayName;

type ToastProps = React.ComponentPropsWithoutRef<typeof Toast>;

type ToastActionElement = React.ReactElement<typeof ToastAction>;

export {
  type ToastProps,
  type ToastActionElement,
  ToastProvider,
  ToastViewport,
  Toast,
  ToastTitle,
  ToastDescription,
  ToastClose,
  ToastAction
};

