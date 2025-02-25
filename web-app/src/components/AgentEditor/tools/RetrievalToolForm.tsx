import Button from "@/components/ui/Button";
import { RetrievalTool } from "../type";
import { useFieldArray, useForm } from "react-hook-form";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import Textarea from "@/components/ui/Form/TextArea";
import Icon from "@/components/ui/Icon";
import { formContentStyles, formWrapperStyles } from "../styles";
import ModalFormFooter from "../ModalFormFooter";
import { vstack, hstack } from "styled-system/patterns";
import Text from "@/components/ui/Typography/Text";
import SourceFieldItem from "../SourceFieldItem";

interface RetrievalToolFormData {
  type: "retrieval";
  name: string;
  src: { value: string }[];
  description?: string;
  api_key?: string;
  api_url?: string;
  embed_model?: string;
  factor?: number;
  key_var?: string;
  n_dims?: number;
  top_k?: number;
}

const RetrievalToolForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: RetrievalTool | null;
  onUpdate: (data: RetrievalTool) => void;
  onCancel: () => void;
}) => {
  const defaultValues: RetrievalToolFormData = value
    ? {
        type: "retrieval",
        name: value.name,
        src: value.src.map((src) => ({ value: src })),
        description: value.description,
        api_key: value.api_key ?? undefined,
        api_url: value.api_url,
        embed_model: value.embed_model,
        factor: value.factor,
        key_var: value.key_var,
        n_dims: value.n_dims,
        top_k: value.top_k,
      }
    : {
        type: "retrieval",
        name: "",
        src: [],
        description:
          "Retrieve the relevant SQL queries to support query generation.",
        embed_model: "text-embedding-3-small",
        factor: 5,
        key_var: "OPENAI_API_KEY",
        n_dims: 512,
        top_k: 4,
        api_url: "https://api.openai.com/v1",
      };

  const {
    handleSubmit,
    register,
    control,
    formState: { errors },
  } = useForm<RetrievalToolFormData>({
    defaultValues,
  });

  const {
    fields,
    append: appendSource,
    remove: removeSource,
  } = useFieldArray({
    name: "src",
    control,
  });

  const onSubmit = async (data: RetrievalToolFormData) => {
    onUpdate({
      ...data,
      src: data.src.map((src) => src.value),
    });
  };

  const onAddSource = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    appendSource({ value: "source" });
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
          label={
            <div
              className={hstack({
                justifyContent: "space-between",
                justify: "center",
              })}
            >
              <Text variant="label14Medium" color="primary">
                Files
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
                  remove={removeSource}
                  error={
                    errors.src?.[index]?.value?.message as string | undefined
                  }
                />
              ))}
            </div>
          )}
        </FormField>

        <FormField name="description" label="Description">
          {() => <Textarea {...register("description")} />}
        </FormField>

        <FormField name="embed_model" label="Embed Model">
          {() => <TextFieldInput {...register("embed_model")} />}
        </FormField>

        <FormField name="api_key" label="API Key">
          {() => <TextFieldInput {...register("api_key")} />}
        </FormField>

        <FormField name="api_url" label="API URL">
          {() => <TextFieldInput {...register("api_url")} />}
        </FormField>

        <FormField name="factor" label="Factor">
          {() => (
            <TextFieldInput
              type="number"
              {...register("factor", { valueAsNumber: true })}
            />
          )}
        </FormField>

        <FormField name="key_var" label="Key Variable">
          {() => <TextFieldInput {...register("key_var")} />}
        </FormField>

        <FormField name="n_dims" label="Number of Dimensions">
          {() => (
            <TextFieldInput
              type="number"
              {...register("n_dims", { valueAsNumber: true })}
            />
          )}
        </FormField>

        <FormField name="top_k" label="Top K">
          {() => (
            <TextFieldInput
              type="number"
              {...register("top_k", { valueAsNumber: true })}
            />
          )}
        </FormField>
      </div>

      <ModalFormFooter onCancel={onCancel} isUpdate={!!value} />
    </form>
  );
};

export default RetrievalToolForm;
