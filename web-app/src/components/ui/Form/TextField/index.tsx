"use client";

import React, { useContext, useMemo } from "react";
import { cx, RecipeVariantProps } from "styled-system/css";
import { composeEventHandlers } from "@radix-ui/primitive";
import { textFieldStyles } from "./TextField.styles";

type TextFieldStyledProps = RecipeVariantProps<typeof textFieldStyles>;

const TextFieldContext = React.createContext<TextFieldStyledProps | undefined>(
  undefined,
);

type TextFieldInputElement = React.ElementRef<"input">;
type TextFieldInputProps = React.ComponentPropsWithRef<"input"> &
  TextFieldStyledProps & { rootClassName?: string };

const TextFieldInput = React.forwardRef<
  TextFieldInputElement,
  TextFieldInputProps
>(function TextFieldInput(props, forwardedRef) {
  const {
    className,
    disabled,
    state = "default",
    slotVariant = "default",
    rootClassName,
    ...inputProps
  } = props;
  const context = useContext(TextFieldContext);
  const hasRoot = context !== undefined;
  const styles = textFieldStyles({
    state: context?.state ?? state,
    slotVariant: context?.slotVariant ?? slotVariant,
  });

  const input = (
    <>
      <input
        data-form-element="input"
        type="text"
        aria-disabled={disabled}
        spellCheck="false"
        disabled={disabled}
        {...inputProps}
        ref={forwardedRef}
        className={cx(styles.input, "text-field-input", "peer", className)}
      />
      <div className={cx(styles["inputField"], "text-field")} />
    </>
  );

  return hasRoot ? (
    input
  ) : (
    <TextFieldRoot
      className={rootClassName}
      disabled={disabled}
      state={state}
      slotVariant={slotVariant}
    >
      {input}
    </TextFieldRoot>
  );
});
type TextFieldRootElement = React.ElementRef<"div">;
type TextFieldRootProps = React.ComponentPropsWithRef<"div"> &
  TextFieldStyledProps & {
    disabled?: boolean;
  };

const TextFieldRoot = React.forwardRef<
  TextFieldRootElement,
  TextFieldRootProps
>(function TextFieldRoot(props, forwardedRef) {
  const { children, className, state, disabled, slotVariant, ...rootProps } =
    props;
  const styles = textFieldStyles({ state, slotVariant });

  const value = useMemo(() => {
    return { state, slotVariant };
  }, [state, slotVariant]);

  return (
    <div
      ref={forwardedRef}
      aria-disabled={disabled}
      {...rootProps}
      onPointerDown={composeEventHandlers(rootProps.onPointerDown, (event) => {
        const target = event.target as HTMLElement;
        if (target.closest("input, button, a")) return;

        const input = event.currentTarget.querySelector(
          ".text-field-input",
        ) as HTMLInputElement | null;
        if (!input) return;

        const position = input.compareDocumentPosition(target);
        const targetIsBeforeInput =
          (position & Node.DOCUMENT_POSITION_PRECEDING) !== 0;
        const cursorPosition = targetIsBeforeInput ? 0 : input.value.length;

        requestAnimationFrame(() => {
          input.setSelectionRange(cursorPosition, cursorPosition);
          input.focus();
        });
      })}
      className={cx(styles.root, className)}
    >
      <TextFieldContext.Provider value={value}>
        {children}
      </TextFieldContext.Provider>
    </div>
  );
});

type TextFieldSlotElement = React.ElementRef<"div">;
type TextFieldSlotProps = React.ComponentPropsWithRef<"div">;

const TextFieldSlot = React.forwardRef<
  TextFieldSlotElement,
  TextFieldSlotProps
>(function TextFieldSlot(props, forwardedRef) {
  const { className, ...slotProps } = props;
  const context = useContext(TextFieldContext);
  const styles = textFieldStyles({
    state: context?.state || "default",
    slotVariant: context?.slotVariant || "default",
  });

  return (
    <div
      ref={forwardedRef}
      {...slotProps}
      className={cx(styles.slot, className)}
    />
  );
});

export { TextFieldInput, TextFieldRoot, TextFieldSlot };
