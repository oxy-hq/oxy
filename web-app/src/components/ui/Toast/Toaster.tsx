"use client";

import { ToastClose } from "@radix-ui/react-toast";
import { css } from "styled-system/css";
import { stack } from "styled-system/patterns";

import StatusIndicator from "../StatusIndicator";
import {
  Toast,
  ToastDescription,
  ToastProvider,
  ToastTitle,
  ToastViewport,
} from "./Toast";
import { useToast } from "./useToast";

const headerStyles = stack({
  w: "300px",
  gap: "xs",
});

const containerStyles = css({
  display: "flex",
  flexDirection: "row",
  gap: "xs",
});

// This is to make the close button cover the entire toast
// User can click anywhere on the toast to close it
const closeStyles = css({
  position: "absolute",
  top: "0",
  right: "0",
  bottom: "0",
  left: "0",
});

export default function Toaster() {
  const { toasts } = useToast();

  return (
    <ToastProvider duration={5000}>
      {toasts.map(function ({ id, title, description, status, ...props }) {
        return (
          <Toast key={id} {...props}>
            <div className={containerStyles}>
              {status && <StatusIndicator status={status} type="icon" />}
              <div className={headerStyles}>
                {title && <ToastTitle>{title}</ToastTitle>}
                {description && (
                  <ToastDescription>{description}</ToastDescription>
                )}
              </div>
            </div>
            <ToastClose asChild>
              <div className={closeStyles} />
            </ToastClose>
          </Toast>
        );
      })}
      <ToastViewport />
    </ToastProvider>
  );
}
