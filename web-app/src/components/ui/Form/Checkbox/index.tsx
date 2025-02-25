"use client";

import { css } from "styled-system/css";
import { PropsWithChildren } from "react";
import Icon from "@/components/ui/Icon";
import React from "react";

const checkboxStyles = css({
  width: "md",
  height: "md",
  padding: "5px 4px",
  borderRadius: "4px",
  display: "flex",
  justifyContent: "center",
  alignItems: "center",
  border: "1px solid",
  borderColor: "border.primary",
  bg: "surface.primary",
  "&[data-state=checked]": {
    bg: "surface.contrast",
    color: "text.contrast"
  },

  flexShrink: 0
});

const checkedStyled = css({ width: "8px!", height: "6px!" });

interface CheckBoxProps {
  onChange: (value: boolean) => void;
  children: React.ReactNode;
  value: boolean;
}

const CheckBox = React.forwardRef<HTMLInputElement, CheckBoxProps>(
  (
    { children, value, onChange, ...props }: PropsWithChildren<CheckBoxProps>,
    ref
  ) => {
    return (
      <div
        className={css({
          display: "flex",
          gap: "xs",
          cursor: "pointer",
          alignItems: "center",
          padding: "sm",
          height: "3xl",
          borderRadius: "rounded",
          "&:hover": {
            backgroundColor: "surface.secondary"
          }
        })}
        onClick={() => {
          onChange(!value);
        }}
      >
        <div data-state={value ? "checked" : ""} className={checkboxStyles}>
          {value && <Icon asset="checkbox" className={checkedStyled} />}
        </div>
        <input
          {...props}
          type="checkbox"
          hidden
          ref={ref}
        />
        {children}
      </div>
    );
  }
);

CheckBox.displayName = "CheckBox";

export default CheckBox;
