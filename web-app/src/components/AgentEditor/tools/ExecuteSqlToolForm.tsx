import { ExecuteSqlTool } from "../type";
import { useForm } from "react-hook-form";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import Textarea from "@/components/ui/Form/TextArea";
import { formContentStyles } from "../styles";
import ModalFormFooter from "../ModalFormFooter";
import { formWrapperStyles } from "../styles";

const ExecuteSqlToolForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: ExecuteSqlTool | null;
  onUpdate: (data: ExecuteSqlTool) => void;
  onCancel: () => void;
}) => {
  const {
    handleSubmit,
    register,
    formState: { errors },
  } = useForm<ExecuteSqlTool>({
    defaultValues: value ?? {
      type: "execute_sql",
      name: "",
      database: "",
      description:
        "Execute the SQL query. If the query is invalid, fix it and run again.",
    },
  });

  const onSubmit = async (data: ExecuteSqlTool) => {
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
          name="database"
          errorMessage={errors.database?.message}
          label="Database *"
        >
          {() => (
            <TextFieldInput
              {...register("database", {
                required: "Database is required",
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

export default ExecuteSqlToolForm;
