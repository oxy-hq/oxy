import { useEffect, useMemo } from "react";

const readTextFile = async (path: string) => {
  const [handle] = await window.showOpenFilePicker({
    suggestedName: path,
  });
  const file = await handle.getFile();
  return await file.text();
};

import { v4 as uuidv4 } from "uuid";

import { useParams } from "react-router-dom";
import { parse } from "yaml";

import WorkflowDiagram from "./WorkflowDiagram";
import { css } from "styled-system/css";
import useProjectPath from "@/stores/useProjectPath";
import Text from "@/components/ui/Typography/Text";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflow, {
  TaskConfig,
  WorkflowConfig,
  TaskType,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import RightSidebar from "./RightSidebar";

const addTaskId = (tasks: TaskConfig[]): TaskConfigWithId[] => {
  return tasks.map((task) => {
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return {
        ...task,
        type: TaskType.LOOP_SEQUENTIAL,
        tasks: addTaskId(task.tasks),
        id: uuidv4(),
      };
    }
    return { ...task, id: uuidv4() } as TaskConfigWithId;
  });
};

const WorkflowPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = path.replace(projectPath, "").replace(/^\//, "");
  const workflow = useWorkflow((state) => state.workflow);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);
  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);
  useEffect(() => {
    setSelectedNodeId(null);
  }, [setSelectedNodeId]);
  useEffect(() => {
    const fetchWorkflow = async () => {
      const workflowText = await readTextFile(path);
      const workflowObj = parse(workflowText) as WorkflowConfig;
      const tasks = addTaskId(workflowObj.tasks);
      const workflow = { ...workflowObj, tasks, path };
      setWorkflow(workflow);
    };

    fetchWorkflow();
    setSelectedNodeId(null);
  }, [path, setSelectedNodeId, setWorkflow]);

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
      <div
        className={css({
          display: "flex",
          height: "100%",
        })}
      >
        <div
          className={css({
            flex: 1,
            height: "100%",
          })}
        >
          <ReactFlowProvider>
            <WorkflowDiagram tasks={workflow.tasks} />
          </ReactFlowProvider>
        </div>
        <RightSidebar key={pathb64} />
      </div>
    </div>
  );
};

export default WorkflowPage;
