import { css } from "styled-system/css";
import { TabList, Tabs, Tab, TabPanel } from "@/components/ui/Tabs";
import { useFormContext } from "react-hook-form";
import Text from "@/components/ui/Typography/Text";
import FileSelectField from "@/components/ui/Form/FileSelectField";
import { TextAreaField } from "./TextAreaField";
import { ExecuteSqlTaskConfig } from "@/stores/useWorkflow";

interface SqlTabProps {
  projectPath: string;
  task: ExecuteSqlTaskConfig;
}

const SqlTab: React.FC<SqlTabProps> = ({ projectPath, task }) => {
  const { register } = useFormContext();

  return (
    <Tabs defaultValue="sql_file">
      <TabList
        className={css({
          display: "flex",
          gap: "lg",
        })}
      >
        <Tab
          className={css({
            "&[data-state=active]": {
              boxShadow: "none !important",
            },
            "&:hover": {
              boxShadow: "none",
            },
            padding: 0,
          })}
          value="sql_file"
        >
          <Text variant="tabBase">SQL File</Text>
        </Tab>
        <Tab
          className={css({
            "&[data-state=active]": {
              boxShadow: "none !important",
            },
            "&:hover": {
              boxShadow: "none",
            },
            padding: 0,
          })}
          value="sqlCode"
        >
          <Text variant="tabBase">Plain code</Text>
        </Tab>
      </TabList>
      <TabPanel value="sql_file">
        <FileSelectField
          basePath={projectPath}
          placeholder="Click to select file"
          {...register("sqlFile")}
          defaultValue={task.sql_file}
        />
      </TabPanel>
      <TabPanel value="sqlCode">
        <TextAreaField
          value={task.sql}
          placeholder="SQL code"
          {...register("sqlCode")}
        />
      </TabPanel>
    </Tabs>
  );
};

export default SqlTab;
