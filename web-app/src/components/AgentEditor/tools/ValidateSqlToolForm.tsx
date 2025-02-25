import { ValidateSqlTool } from "../type";
import { useForm } from "react-hook-form";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import Textarea from "@/components/ui/Form/TextArea";
import { formContentStyles, formWrapperStyles } from "../styles";
import ModalFormFooter from "../ModalFormFooter";

const ValidateSqlToolForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: ValidateSqlTool | null;
  onUpdate: (data: ValidateSqlTool) => void;
  onCancel: () => void;
}) => {
  const {
    handleSubmit,
    register,
    formState: { errors },
  } = useForm<ValidateSqlTool>({
    defaultValues: value ?? {
      type: "validate_sql",
      name: "",
      warehouse: "",
      description:
        "Validate the SQL query. If the query is invalid, fix it and run again.",
    },
  });

  const onSubmit = async (data: ValidateSqlTool) => {
    onUpdate(data);
  };

  return (
    <form className={formWrapperStyles} onSubmit={handleSubmit(onSubmit)}>
      <div className={formContentStyles}>
        <FormField
          name="name"
          errorMessage={errors.name?.message}
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
          name="warehouse"
          errorMessage={errors.warehouse?.message}
          label="Warehouse *"
        >
          {() => (
            <TextFieldInput
              {...register("warehouse", {
                required: "Warehouse is required",
              })}
            />
          )}
        </FormField>

        <FormField name="description" label="Description">
          {() => <Textarea {...register("description")} />}
        </FormField>
      </div>
      <ModalFormFooter onCancel={onCancel} isUpdate={!!value} />
    </form>
  );
};

export default ValidateSqlToolForm;
