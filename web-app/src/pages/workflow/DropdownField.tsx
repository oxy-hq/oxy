import { css } from "styled-system/css";
import Select from "./Select";
import FieldLabel from "./FieldLabel";

type Props = {
  label: string;
  options: { value: string; label: string }[];
  inline?: boolean;
};

const DropdownField = ({ label, options, inline, ...props }: Props) => {
  const containerStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: "10px",
  });

  const inlineLineStyle = css({
    display: "flex",
    gap: "16px",
    alignItems: "center",
    justifyContent: "space-between",
  });
  return (
    <div className={inline ? inlineLineStyle : containerStyle}>
      <FieldLabel>{label}</FieldLabel>
      <div
        className={css({
          // padding: "sm",
          // backgroundColor: "rgba(0, 0, 0, 0.02)",
          borderRadius: "16px",
          flex: 1,
        })}
      >
        <Select options={options} {...props} />
      </div>
    </div>
  );
};

export default DropdownField;
