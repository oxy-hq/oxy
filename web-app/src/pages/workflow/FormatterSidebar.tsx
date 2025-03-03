import { FormProvider, useForm, useWatch } from "react-hook-form";
import { SideBarContainer } from "./SideBarContainer";
import SideBarContent from "./SideBarContent";
import SideBarStepHeader from "./SideBarStepHeader";
import { TextAreaField } from "./TextAreaField";
import { TextFieldWithLabel } from "./TextFieldWithLabel";
import ExportSection from "./ExportSection";
import StepDataContainer from "./StepDataContainer";
import { useEffect } from "react";
import useWorkflow, { ExportConfig, Node } from "@/stores/useWorkflow";

type FormatterStepData = {
  name: string;
  template: string;
  export?: ExportConfig;
};

type Props = {
  node: Node;
};

export const FormatterSidebar = ({ node }: Props) => {
  const task = node.data.task as FormatterStepData;
  const defaultValues: FormatterStepData = {
    name: task.name,
    template: task.template,
    export: task.export,
  };
  const methods = useForm<FormatterStepData>({
    defaultValues,
    mode: "onChange",
  });
  const updateStep = useWorkflow((state) => state.updateTask);
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);

  const id = node.data.task.id;

  const { register } = methods;
  const formValues = useWatch({
    control: methods.control,
  }) as FormatterStepData;

  useEffect(() => {
    const updateData: Partial<FormatterStepData> = {
      name: formValues.name,
      template: formValues.template,
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
              <TextAreaField label="Template" {...register("template")} />
            </StepDataContainer>
            <ExportSection />
          </form>
        </FormProvider>
      </SideBarContent>
    </SideBarContainer>
  );
};
