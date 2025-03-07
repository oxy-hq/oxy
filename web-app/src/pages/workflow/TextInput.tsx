import { css } from "styled-system/css";

export const TextInput = ({ ...props }) => {
  return (
    <div
      className={css({
        padding: "sm",
        backgroundColor: "rgba(0, 0, 0, 0.02)",
        borderRadius: "8px",
      })}
    >
      <input
        className={css({
          fontSize: "14px",
          border: "none",
          fontFamily: "Inter",
          lineHeight: "17px",
          fontWeight: 400,
          outline: "none",
        })}
        {...props}
      ></input>
    </div>
  );
};
