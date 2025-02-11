import { useEffect, useState } from "react";

import { readTextFile } from "@tauri-apps/plugin-fs";
import { useParams } from "react-router-dom";
import { parse } from "yaml";

import WorkflowDiagram from "./WorkflowDiagram";
import { css } from "styled-system/css";
import useProjectPath from "@/stores/useProjectPath";
import Text from "@/components/ui/Typography/Text";
import { ReactFlowProvider } from "@xyflow/react";

export type Workflow = {
  name: string;
  steps: StepData[];
};

const WorkflowPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = atob(pathb64);
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = path.replace(projectPath, "").replace(/^\//, "");
  const [workflow, setWorkflow] = useState<Workflow | null>(null);
  useEffect(() => {
    const fetchWorkflow = async () => {
      const workflow = await readTextFile(path);
      setWorkflow(parse(workflow));
    };
    fetchWorkflow();
  }, [path]);
  if (workflow === null) {
    return <div>Loading...</div>;
  }
  return (
    <div
      className={css({
        width: "100%",
        height: "100%",
      })}
    >
      <div
        className={css({
          padding: "sm",
          border: "1px solid",
          borderColor: "neutral.border.colorBorderSecondary",
          backgroundColor: "neutral.bg.colorBg",
        })}
      >
        <Text variant="bodyBaseMedium">{relativePath}</Text>
      </div>
      <ReactFlowProvider>
        <WorkflowDiagram steps={workflow.steps} />
      </ReactFlowProvider>
    </div>
  );
};

export type StepData = {
  id: string;
  name: string;
  type: string;
};

export default WorkflowPage;
