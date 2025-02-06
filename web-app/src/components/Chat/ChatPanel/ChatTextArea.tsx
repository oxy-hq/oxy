"use client";

import { useEffect, useRef } from "react";

import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Spinner from "@/components/ui/Spinner";

import { TextArea, TextAreaRoot } from "../TextArea";

const textAreaRootStyles = css({
  transition: "box-shadow 0.2s ease-in-out",
});

const containerStyles = css({
  // border and shadow
  shadow: "0 0 0 1px token(colors.border.primary), token(shadows.primary)",
  _focusWithin: {
    // border and shadow
    shadow: "0 0 0 1px token(colors.border.primary), token(shadows.secondary)",
  },
  borderRadius: "full",
  width: "100%",
  display: "flex",
  flexDirection: "row",
  gap: "md",
  backgroundColor: "surface.primary",
  position: "relative",
  py: "sm",
  pr: "sm",
  pl: "lg",
  justifyContent: "space-between",
  alignItems: "center",
  transition: "box-shadow 0.2s ease-in-out",
});

export interface ChatTextAreaProps {
  hasMessage: boolean;
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  pending: boolean;
  botName?: string;
  disabled?: boolean;
}
export default function ChatTextArea({
  onKeyDown,
  pending,
  botName = "Onyx AI",
  disabled = false,
}: ChatTextAreaProps) {
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!pending) {
      inputRef.current?.focus();
    }
  }, [pending]);

  const placeholder = `Message ${botName}`;

  return (
    <div className={containerStyles}>
      <TextAreaRoot variant="default" className={textAreaRootStyles}>
        <TextArea
          ref={inputRef}
          name="content"
          placeholder={placeholder}
          disabled={pending || disabled}
          onKeyDown={onKeyDown}
          className={css({
            height: "20px!",
          })}
          autoFocus
        />
      </TextAreaRoot>
      <Button
        size="large"
        content="icon"
        variant="outline"
        type="submit"
        disabled={pending || disabled}
      >
        {pending ? <Spinner /> : <Icon asset="arrow_up" />}
      </Button>
    </div>
  );
}
