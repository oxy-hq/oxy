import { AgentContextSemanticModel } from "@/components/AgentEditor/type";
import { useForm } from "react-hook-form";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import ModalFormFooter from "../ModalFormFooter";
import { formContentStyles, formWrapperStyles } from "../styles";

const AgentContextSemanticModelForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: AgentContextSemanticModel | null;
  onUpdate: (data: AgentContextSemanticModel) => void;
  onCancel: () => void;
}) => {
  const {
    handleSubmit,
    register,
    formState: { errors },
  } = useForm<AgentContextSemanticModel>({
    defaultValues: value ?? {
      type: "semantic_model",
      name: "",
      src: "",
    },
  });

  const onSubmit = async (data: AgentContextSemanticModel) => {
    onUpdate(data);
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
          name="src"
          errorMessage={errors.src?.message as string | undefined}
          label="Source *"
        >
          {() => (
            <TextFieldInput
              {...register("src", {
                required: "Source is required",
              })}
            />
          )}
        </FormField>
      </div>

      <ModalFormFooter onCancel={onCancel} isUpdate={!!value} />
    </form>
  );
};

export default AgentContextSemanticModelForm;
