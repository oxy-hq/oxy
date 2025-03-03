import { css } from "styled-system/css";
import { TextFieldInput, TextFieldRoot } from "@/components/ui/Form/TextField";

const FormTextInput: React.FC = (props) => {
  return (
    <TextFieldRoot
      className={css({
        flex: 1,
        backgroundColor: "rgba(0, 0, 0, 0.02)",
        borderRadius: "rounded",
        boxSizing: "border-box",
      })}
    >
      <TextFieldInput
        className={css({
          backgroundColor: "rgba(0, 0, 0, 0.02)",
          borderRadius: "rounded",
        })}
        {...props}
      />
    </TextFieldRoot>
  );
};

export default FormTextInput;
