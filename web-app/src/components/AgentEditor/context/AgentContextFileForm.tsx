import Button from "@/components/ui/Button";
import { AgentContextFile } from "@/components/AgentEditor/type";
import { useFieldArray, useForm } from "react-hook-form";
import { hstack, vstack } from "styled-system/patterns";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import { formContentStyles, formWrapperStyles } from "../styles";
import ModalFormFooter from "../ModalFormFooter";
import SourceFieldItem from "../SourceFieldItem";
export interface AgentContextFileFormData {
  type: "file";
  name: string;
  src: {
    value: string;
  }[];
}

const AgentContextFileForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: AgentContextFile | null;
  onUpdate: (data: AgentContextFile) => void;
  onCancel: () => void;
}) => {
  const defaultValues: AgentContextFileFormData = value
    ? {
        type: "file",
        name: value.name,
        src: value.src.map((src) => ({ value: src })),
      }
    : {
        type: "file",
        name: "",
        src: [],
      };

  const {
    handleSubmit,
    register,
    control,
    formState: { errors },
    setError,
    clearErrors,
  } = useForm<AgentContextFileFormData>({
    defaultValues,
  });

  const { fields, append, remove } = useFieldArray({
    name: "src",
    control,
  });

  const onSubmit = async (data: AgentContextFileFormData) => {
    if (data.src.length === 0) {
      setError("src", {
        message: "At least one file is required",
      });
      return;
    }
    onUpdate({
      type: "file",
      name: data.name,
      src: data.src.map((src) => src.value),
    });
  };

  const onAddSource = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    append({ value: "source" });
    clearErrors("src");
  };

  return (
    <form className={formWrapperStyles} onSubmit={handleSubmit(onSubmit)}>
      <div className={formContentStyles}>
        <FormField
          name="name"
          errorMessage={errors.name?.message as string | undefined}
          label="Name *"
        >
          {() => (
            <TextFieldInput
              {...register("name", {
                required: "Name is required",
              })}
            />
          )}
        </FormField>

        <FormField
          label={
            <div
              className={hstack({
                justifyContent: "space-between",
                justify: "center",
              })}
            >
              <Text variant="label14Medium" color="primary">
                Files *
              </Text>
              <Button
                type="button"
                variant="ghost"
                content="icon"
                onClick={onAddSource}
              >
                <Icon asset="add" />
              </Button>
            </div>
          }
          errorMessage={errors.src?.message as string | undefined}
          name="src"
        >
          {() => (
            <div className={vstack({ gap: "sm", alignItems: "stretch" })}>
              {fields.map((field, index) => (
                <SourceFieldItem
                  key={field.id}
                  id={field.id}
                  index={index}
                  registerReturn={register(`src.${index}.value`, {
                    required: "File is required",
                  })}
                  remove={remove}
                  error={
                    errors.src?.[index]?.value?.message as string | undefined
                  }
                />
              ))}
            </div>
          )}
        </FormField>
      </div>

      <ModalFormFooter onCancel={onCancel} isUpdate={!!value} />
    </form>
  );
};

export default AgentContextFileForm;
