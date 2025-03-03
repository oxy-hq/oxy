import { css } from "styled-system/css";
import FieldLabel from "./FieldLabel";

interface TextAreaFieldProps {
  label?: string;
  placeholder?: string;
}

export const TextAreaField: React.FC<TextAreaFieldProps> = ({ label, placeholder, ...props }) => {
  return (
    <div
      className={css({ display: "flex", flexDirection: "column", gap: "10px" })}
    >
      {label ? <FieldLabel>{label}</FieldLabel> : null}
      <div
        className={css({
          padding: "sm",
          backgroundColor: "rgba(0, 0, 0, 0.02)",
          borderRadius: "8px",
        })}
      >
        <textarea
          placeholder={placeholder}
          className={css({
            fontSize: "14px",
            border: "none",
            fontFamily: "Inter",
            lineHeight: "17px",
            fontWeight: 400,
            outline: "none",
            resize: "none",
            width: "100%",
            height: "100px",
          })}
          {...props}
        ></textarea>
      </div>
    </div>
  );
};
