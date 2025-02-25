import { css } from "styled-system/css";
import Text from "@/components/ui/Typography/Text";
import { useCallback, useEffect, useMemo } from "react";
import { useState } from "react";
import { AgentConfig } from "./type";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import yaml from "yaml";
import { parse } from "yaml";
import FormField from "../ui/Form/FormField";
import { TextFieldInput } from "../ui/Form/TextField";
import Textarea from "../ui/Form/TextArea";
import { Controller, FormProvider, useForm } from "react-hook-form";
import {
  SelectRoot as Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/Form/SelectField";
import { debounce } from "lodash";
import AnonymizerField from "./anonymizer/AnonymizerField";
import ContextField from "./context/ContextField";
import ToolField from "./tools/ToolField";

const wrapperStyles = css({
  flex: "1",
  height: "100%",
  bg: "white",
  p: "4xl",
  gap: "4xl",
  display: "flex",
  flexDir: "column",
  alignItems: "stretch",
  h: "100%",
  overflow: "auto",
  customScrollbar: true,
});

const AgentEditor = ({ path }: { path: string }) => {
  const agentName = path.split("/").pop();
  const [loading, setLoading] = useState(false);
  const [agent, setAgent] = useState<AgentConfig | null>(null);
  const methods = useForm<AgentConfig>({
    mode: "onChange",
  });
  const {
    register,
    control,
    reset,
    watch,
    handleSubmit,
    formState: { errors },
  } = methods;

  useEffect(() => {
    const fetchAgent = async () => {
      setLoading(true);
      const agent = await readTextFile(path);
      const parsedAgent = parse(agent);
      setAgent(parsedAgent);
      reset(parsedAgent);
      setLoading(false);
    };
    fetchAgent();
  }, [path, reset]);

  const onSave = useCallback(() => {
    handleSubmit((data) => {
      const updatedAgent = { ...agent, ...data };
      if (updatedAgent.context && updatedAgent.context.length === 0) {
        updatedAgent.context = undefined;
      }
      if (updatedAgent.tools && updatedAgent.tools.length === 0) {
        updatedAgent.tools = undefined;
      }
      writeTextFile(path, yaml.stringify(updatedAgent));
    })();
  }, [path, handleSubmit, agent]);

  const debouncedSave = useMemo(() => debounce(onSave, 1000), [onSave]);

  useEffect(() => {
    const { unsubscribe } = watch(() => {
      debouncedSave();
    });
    return () => unsubscribe();
  }, [watch, debouncedSave]);

  if (loading) {
    return <div>Saving...</div>;
  }

  console.log(agent);

  return (
    <FormProvider {...methods}>
      <div className={wrapperStyles}>
        <Text variant="headingH4" className={css({ textAlign: "center" })}>
          Editor
        </Text>

        <FormField label="Agent name" name="name">
          {() => (
            <TextFieldInput
              disabled
              rootClassName={css({
                maxW: "100%!",
                w: "100%!",
              })}
              value={agentName}
            />
          )}
        </FormField>

        <FormField
          label="Model *"
          name="model"
          errorMessage={errors.model?.message}
        >
          {() => (
            <TextFieldInput
              rootClassName={css({
                maxW: "100%!",
                w: "100%!",
              })}
              {...register("model", { required: "Model is required" })}
            />
          )}
        </FormField>

        <FormField
          label="System instructions *"
          name="system_instructions"
          errorMessage={errors.system_instructions?.message}
        >
          {() => (
            <Textarea
              className={css({
                maxW: "100%!",
                w: "100%!",
              })}
              {...register("system_instructions", {
                required: "System instructions are required",
              })}
            />
          )}
        </FormField>

        <AnonymizerField />

        <ContextField />

        <ToolField />

        <FormField name="output_format" label="Output format">
          {() => (
            <Controller
              control={control}
              name="output_format"
              render={({ field: { onChange, value } }) => (
                <Select onValueChange={onChange} value={value}>
                  <SelectTrigger
                    className={css({
                      maxW: "100%!",
                      w: "100%!",
                    })}
                  >
                    <SelectValue placeholder="Output format" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">Default</SelectItem>
                    <SelectItem value="file">File</SelectItem>
                  </SelectContent>
                </Select>
              )}
            />
          )}
        </FormField>
      </div>
    </FormProvider>
  );
};

export default AgentEditor;
