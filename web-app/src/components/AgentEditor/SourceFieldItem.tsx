import Button from "@/components/ui/Button";
import { UseFormRegisterReturn } from "react-hook-form";
import { hstack } from "styled-system/patterns";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import Icon from "@/components/ui/Icon";
import { css } from "styled-system/css";

const SourceFieldItem = ({
  id,
  index,
  registerReturn,
  remove,
  error,
}: {
  id: string;
  index: number;
  registerReturn: UseFormRegisterReturn;
  remove: (index: number) => void;
  error: string | undefined;
}) => {
  return (
    <FormField name={`src.${index}.value`} errorMessage={error}>
      {() => (
        <div key={id} className={hstack({ gap: "sm" })}>
          <TextFieldInput
            rootClassName={css({ flex: 1 })}
            {...registerReturn}
          />
          <Button
            type="button"
            variant="ghost"
            content="icon"
            onClick={() => remove(index)}
          >
            <Icon asset="remove_minus" />
          </Button>
        </div>
      )}
    </FormField>
  );
};

export default SourceFieldItem;
