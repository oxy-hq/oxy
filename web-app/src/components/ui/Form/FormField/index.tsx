import { css } from "styled-system/css";
import { useId } from "react";
import Text from "../../Typography/Text";

const errorStyles = css({
  textStyle: "label14Regular",
  color: "text.error",
});

const fieldStyles = css({
  display: "flex",
  flexDirection: "column",
  gap: "sm",
  width: "100%",
});

const labelStyles = css({
  color: "text.primary",
  textStyle: "label14Medium",
});

const descriptionStyles = css({
  display: "flex",
  color: "text.secondary!",
  mt: "xs",
});

interface FormFieldChildProps {
  state: "default" | "error";
  name: string;
  id: string;
  "aria-describedby": string | undefined;
}

interface FormFieldProps {
  children: (innerProps: FormFieldChildProps) => React.ReactNode;
  errorMessage?: string | string[] | null;
  name: string;
  label?: string | React.ReactNode;
  description?: string;
}

export default function FormField({
  children,
  errorMessage,
  name,
  label,
  description,
}: FormFieldProps) {
  const fieldId = useId();
  const errorId = errorMessage ? `${fieldId}-error` : undefined;
  const state = errorMessage ? "error" : "default";

  return (
    <div className={fieldStyles}>
      {label && (
        <label className={labelStyles} htmlFor={fieldId}>
          {label}
          {description && (
            <Text variant="paragraph12Regular" className={descriptionStyles}>
              {description}
            </Text>
          )}
        </label>
      )}
      {children({ state, name, id: fieldId, "aria-describedby": errorId })}
      {errorMessage && (
        <span
          className={errorStyles}
          id={errorId}
          aria-live="polite"
          role="alert"
        >
          {errorMessage}
        </span>
      )}
    </div>
  );
}
