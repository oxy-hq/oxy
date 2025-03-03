import { css } from "styled-system/css";
import { TextInput } from "./TextInput";
import FieldLabel from "./FieldLabel";

type Props = {
    label: string;
}

export const TextFieldWithLabel = ({ label, ...props }: Props) => {
    return (
        <div
            className={css({ display: "flex", flexDirection: "column", gap: "10px" })}
        >
            <FieldLabel>{label}</FieldLabel>
            <TextInput {...props} />
        </div>
    );
};
