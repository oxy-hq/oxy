import { Controller, FormProvider, useForm, useWatch } from "react-hook-form";
import { SideBarContainer } from "./SideBarContainer";
import SideBarContent from "./SideBarContent";
import SideBarStepHeader from "./SideBarStepHeader";
import { TextAreaField } from "./TextAreaField";
import { TextFieldWithLabel } from "./TextFieldWithLabel";
import { useEffect } from "react";
import useWorkflow, {
  AgentTaskConfig,
  ExportConfig,
  Node,
} from "@/stores/useWorkflow";
import StepDataContainer from "./StepDataContainer";
import ExportSection from "./ExportSection";
import { AgentRefSelect } from "./AgentRefSelect"; // Import from new file

type AgentFormFields = {
  name: string;
  prompt: string;
  agent_ref: string;
  export?: ExportConfig;
};

type Props = {
  node: Node;
};

export const AgentSidebar = ({ node }: Props) => {
  const task = node.data.task as AgentTaskConfig;
  const defaultValues: AgentFormFields = {
    name: task.name,
    prompt: task.prompt,
    agent_ref: task.agent_ref,
    export: task.export,
  };
  const methods = useForm<AgentFormFields>({
    defaultValues,
    mode: "onChange",
  });
  const updateStep = useWorkflow((state) => state.updateTask);
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);

  const id = node.data.task.id;

  const { register, control } = methods;
  const formValues = useWatch({ control: methods.control }) as AgentFormFields;

  useEffect(() => {
    const updateData: Partial<AgentFormFields> = {
      name: formValues.name,
      agent_ref: formValues.agent_ref,
      prompt: formValues.prompt,
      export: formValues.export,
    };

    updateStep(id, updateData);
    saveWorkflow();
  }, [formValues, updateStep, saveWorkflow, id]);

  return (
    <SideBarContainer>
      <SideBarStepHeader>Formatter</SideBarStepHeader>
      <SideBarContent>
        <FormProvider {...methods}>
          <form>
            <StepDataContainer>
              <TextFieldWithLabel label="Name" {...register("name")} />
              <TextAreaField label="Prompt" {...register("prompt")} />
              <Controller
                control={control}
                name="agent_ref"
                render={({ field }) => {
                  return (
                    <AgentRefSelect {...field} onValueChange={field.onChange} />
                  );
                }}
              ></Controller>
            </StepDataContainer>
            <ExportSection />
          </form>
        </FormProvider>
      </SideBarContent>
    </SideBarContainer>
  );
};
