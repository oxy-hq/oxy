"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { VisuallyHidden } from "@radix-ui/react-visually-hidden";
import { cx, sva } from "styled-system/css";

import Icon from "../Icon";
import Button from "../Button";

const modalStyles = sva({
  slots: ["overlay", "content", "header", "footer", "description"],
  base: {
    overlay: {
      pos: "fixed",
      inset: 0,
      bg: "background.opacity",
      zIndex: 100, // TODO: need to standardize z-index
    },
    content: {
      pos: "fixed",
      left: "50%",
      top: "50%",
      zIndex: 101, // TODO: need to standardize z-index
      transform: "translate(-50%, -50%)",
      bg: "surface.primary",
      boxShadow: "secondary",
      // TODO: Now that we have different modals, this layout is not standardised
      // we need to find a way to standardise this and make it mobile responsive out of the box
      // width: "600px",
      borderRadius: "rounded",
      border: "1px solid",
      borderColor: "border.primary",
    },
    header: {
      display: "flex",
      direction: "row",
      justifyContent: "space-between",
      alignItems: "center",
      // TODO: should be standardised with modal component,
      // as right now we don't have heading typography
      textStyle: "label16Medium",
      color: "text.primary",
    },
    footer: {
      display: "flex",
      direction: "row",
      justifyContent: "flex-end",
      alignItems: "center",
    },
    description: {
      // TODO: should be standardised with modal component
      textStyle: "label12Regular",
    },
  },
})();

const Modal = DialogPrimitive.Root;

const ModalTrigger = DialogPrimitive.Trigger;

const ModalPortal = DialogPrimitive.Portal;

const ModalClose = DialogPrimitive.Close;

const ModalOverlay = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cx(modalStyles.overlay, className)}
    {...props}
  />
));
ModalOverlay.displayName = DialogPrimitive.Overlay.displayName;

const ModalContent = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & {
    width?: string;
  }
>(({ className, children, ...props }, ref) => (
  <ModalPortal container={document.getElementById("app-root")}>
    <ModalOverlay />
    <DialogPrimitive.Content
      ref={ref}
      className={cx(modalStyles.content, className)}
      {...props}
    >
      {children}
    </DialogPrimitive.Content>
  </ModalPortal>
));
ModalContent.displayName = DialogPrimitive.Content.displayName;

const ModalHeader = ({
  className,
  closable,
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement> & {
  closable?: boolean;
}) => (
  <div className={cx(modalStyles.header, className)} {...props}>
    {children}
    {closable && (
      <ModalClose asChild>
        <Button variant="ghost" content="icon">
          <Icon asset="close" />
        </Button>
      </ModalClose>
    )}
    {/* Title is required by the library
    https://www.radix-ui.com/primitives/docs/components/dialog */}
    <VisuallyHidden asChild>
      <DialogPrimitive.Title />
    </VisuallyHidden>
  </div>
);
ModalHeader.displayName = "ModalHeader";

const ModalFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div className={cx(modalStyles.footer, className)} {...props} />
);
ModalFooter.displayName = "ModalFooter";

const ModalDescription = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cx(modalStyles.description, className)}
    {...props}
  />
));
ModalDescription.displayName = DialogPrimitive.Description.displayName;

export {
  Modal,
  ModalPortal,
  ModalOverlay,
  ModalTrigger,
  ModalClose,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalDescription,
};
