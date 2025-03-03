import { useEffect } from "react";
import useProjectPath from "@/stores/useProjectPath";
import useWorkflow, {
  ExecuteSqlTaskConfig,
  ExportConfig,
  Node,
} from "@/stores/useWorkflow";
import { Controller, FormProvider, useForm, useWatch } from "react-hook-form";
import { SideBarContainer } from "./SideBarContainer";
import { TextFieldWithLabel } from "./TextFieldWithLabel";
import DatabaseSelect from "./WarehouseSelect";
import SideBarStepHeader from "./SideBarStepHeader";
import ExportSection from "./ExportSection";
import SqlTab from "./SqlTab";
import StepDataContainer from "./StepDataContainer"; // Import the new component

export type SqlInputs = {
  name: string;
  database: string;
  sqlFile?: string;
  sqlCode?: string;
  export?: ExportConfig;
};

type ExecuteSqlSidebarProps = {
  node: Node;
};

const ExecuteSqlSidebar: React.FC<ExecuteSqlSidebarProps> = ({ node }) => {
  const projectPath = useProjectPath((state) => state.projectPath);
  const updateStep = useWorkflow((state) => state.updateTask);
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);
  const task = node.data.task;
  if (task.type !== "execute_sql") {
    throw new Error("Invalid task type");
  }

  const id = task.id;

  const defaultValues: SqlInputs = {
    name: task.name,
    database: task.database,
    export: task.export,
    sqlFile: task.sql_file,
    sqlCode: task.sql,
  };

  const methods = useForm<SqlInputs>({
    mode: "onChange",
    defaultValues,
  });

  const { register, control } = methods;
  const formValues = useWatch({ control: methods.control }) as SqlInputs;
  const { sqlFile } = useWatch({ control });
  const { sqlCode } = useWatch({ control });

  useEffect(() => {
    if (sqlFile) {
      methods.setValue("sqlCode", undefined);
    }
  }, [sqlFile, methods]);

  useEffect(() => {
    if (sqlCode) {
      methods.setValue("sqlFile", undefined);
    }
  }, [sqlCode, methods]);

  useEffect(() => {
    const updateData: Partial<ExecuteSqlTaskConfig> = {
      name: formValues.name,
      database: formValues.database,
      sql_file: formValues.sqlFile,
      sql: formValues.sqlCode,
      export: formValues.export,
    };

    updateStep(id!, updateData);
    saveWorkflow();
  }, [formValues, updateStep, saveWorkflow, id]);

  return (
    <SideBarContainer>
      <SideBarStepHeader>SQL</SideBarStepHeader>
      <FormProvider {...methods}>
        <form>
          <StepDataContainer>
            <TextFieldWithLabel label="Name" {...register("name")} />
            <Controller
              name="database"
              control={control}
              render={({ field }) => (
                <DatabaseSelect
                  label="Database"
                  {...field}
                  onValueChange={field.onChange}
                />
              )}
            />
            <SqlTab projectPath={projectPath} task={task} />
          </StepDataContainer>
          <ExportSection />
        </form>
      </FormProvider>
    </SideBarContainer>
  );
};

export default ExecuteSqlSidebar;
