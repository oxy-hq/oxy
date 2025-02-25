import {
  AnonymizerConfig,
  AnonymizerConfigFlashText,
} from "@/components/AgentEditor/type";
import { Controller, useForm } from "react-hook-form";
import FormField from "@/components/ui/Form/FormField";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import CheckBox from "@/components/ui/Form/Checkbox";
import Text from "@/components/ui/Typography/Text";
import {
  SelectRoot as Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/Form/SelectField";
import { formContentStyles } from "../styles";
import ModalFormFooter from "../ModalFormFooter";
import { formWrapperStyles } from "../styles";
import { omit } from "lodash";

type AnonymizerConfigFlashTextWithMode = AnonymizerConfigFlashText & {
  mode: "keywords" | "mapping";
};

const AnonymizerForm = ({
  value,
  onUpdate,
  onCancel,
}: {
  value?: AnonymizerConfig | null;
  onUpdate: (data: AnonymizerConfig) => void;
  onCancel: () => void;
}) => {
  const processedValue: AnonymizerConfigFlashTextWithMode | null = value
    ? {
        ...value,
        mode: value.mapping_file ? "mapping" : "keywords",
      }
    : null;

  const methods = useForm<AnonymizerConfigFlashTextWithMode>({
    defaultValues: processedValue ?? {
      type: "flash_text",
      case_sensitive: false,
      pluralize: false,
      mode: "keywords",
    },
  });

  const {
    handleSubmit,
    register,
    control,
    watch,
    formState: { errors },
  } = methods;

  const mode = watch("mode");

  const onSubmit = async (data: AnonymizerConfigFlashTextWithMode) => {
    const result: AnonymizerConfigFlashText = omit(data, "mode");
    if (data.mode === "keywords") {
      result.mapping_file = "";
      result.delimiter = "";
    } else {
      result.keywords_file = "";
      result.replacement = "";
    }
    onUpdate(result);
  };

  return (
    <form className={formWrapperStyles} onSubmit={handleSubmit(onSubmit)}>
      <div className={formContentStyles}>
        <FormField name="mode" label="Mode">
          {() => (
            <Controller
              control={control}
              name="mode"
              render={({ field: { onChange, value } }) => (
                <Select onValueChange={onChange} value={value}>
                  <SelectTrigger>
                    <SelectValue placeholder="Mode" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="keywords">Keywords</SelectItem>
                    <SelectItem value="mapping">Mapping</SelectItem>
                  </SelectContent>
                </Select>
              )}
            />
          )}
        </FormField>

        {mode === "keywords" && (
          <>
            <FormField
              name="keywords_file"
              errorMessage={errors.keywords_file?.message as string | undefined}
              label="Keywords file *"
            >
              {() => (
                <TextFieldInput
                  {...register("keywords_file", {
                    required: "Keywords file is required",
                  })}
                />
              )}
            </FormField>

            <FormField
              name="replacement"
              errorMessage={errors.replacement?.message as string | undefined}
              label="Replacement"
            >
              {() => <TextFieldInput {...register("replacement")} />}
            </FormField>
          </>
        )}

        {mode === "mapping" && (
          <>
            <FormField
              name="mapping_file"
              errorMessage={errors.mapping_file?.message as string | undefined}
              label="Mapping file *"
            >
              {() => (
                <TextFieldInput
                  {...register("mapping_file", {
                    required: "Mapping file is required",
                  })}
                />
              )}
            </FormField>

            <FormField
              name="delimiter"
              errorMessage={errors.delimiter?.message as string | undefined}
              label="Delimiter"
            >
              {() => <TextFieldInput {...register("delimiter")} />}
            </FormField>
          </>
        )}

        <FormField name="case_sensitive">
          {() => (
            <Controller
              control={control}
              name="case_sensitive"
              render={({ field: { onChange, value } }) => (
                <CheckBox onChange={onChange} value={value ?? false}>
                  <Text variant="label14Medium" color="primary">
                    Case sensitive
                  </Text>
                </CheckBox>
              )}
            />
          )}
        </FormField>

        <FormField name="pluralize">
          {() => (
            <Controller
              control={control}
              name="pluralize"
              render={({ field: { onChange, value } }) => (
                <CheckBox onChange={onChange} value={value ?? false}>
                  <Text variant="label14Medium" color="primary">
                    Pluralize
                  </Text>
                </CheckBox>
              )}
            />
          )}
        </FormField>
      </div>

      <ModalFormFooter onCancel={onCancel} isUpdate={!!value} />
    </form>
  );
};

export default AnonymizerForm;
