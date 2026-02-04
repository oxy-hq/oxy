import type { TaskRun } from "@/services/types";
import {
  type NodeData,
  type NodeType,
  NoneTaskNodeType,
  type TaskConfigWithId,
  TaskType
} from "@/stores/useWorkflow";
import { AgentNode } from "./nodes/AgentNode";
import { ConditionalElseNode } from "./nodes/ConditionalElseNode";
import { ConditionalIfNode } from "./nodes/ConditionalIfNode";
import ConditionalNode from "./nodes/ConditionalNode";
import { ExecuteSqlNode } from "./nodes/ExecuteSqlNode";
import { FormatterNode } from "./nodes/FormatterNode";
import { LoopSequentialNode } from "./nodes/LoopSequentialNode";
import { OmniQueryNode } from "./nodes/OmniQueryNode";
import { SemanticQueryNode } from "./nodes/SemanticQueryNode";
import { WorkflowTaskNode } from "./nodes/WorkflowTaskNode";

type Props = {
  id: string;
  task: TaskConfigWithId;
  type: NodeType;
  data: NodeData;
  parentId?: string;
  taskRun?: TaskRun;
  loopRuns?: TaskRun[];
  width?: number;
  height?: number;
};

export function NodeContent({ parentId, task, type, data, taskRun, loopRuns, ...props }: Props) {
  if (task.type === "loop_sequential") {
    return (
      <LoopSequentialNode
        parentId={parentId}
        task={task}
        taskRun={taskRun}
        loopRuns={loopRuns}
        expanded={data.expanded}
      />
    );
  }
  if (task.type === "execute_sql") {
    return <ExecuteSqlNode task={task} />;
  }
  if (task.type === TaskType.SEMANTIC_QUERY) {
    return <SemanticQueryNode task={task} />;
  }
  if (task.type === TaskType.OMNI_QUERY) {
    return <OmniQueryNode task={task} />;
  }
  if (task.type === "formatter") {
    return <FormatterNode task={task} />;
  }
  if (task.type === "agent") {
    return <AgentNode task={task} />;
  }

  if (task.type === "workflow") {
    return <WorkflowTaskNode task={task} taskRun={taskRun} expanded={data.expanded} />;
  }

  if (type === TaskType.CONDITIONAL) {
    return <ConditionalNode task={task} expanded={data.expanded} />;
  }
  if (type === NoneTaskNodeType.CONDITIONAL_ELSE) {
    return <ConditionalElseNode {...props} />;
  }
  if (type === NoneTaskNodeType.CONDITIONAL_IF) {
    return <ConditionalIfNode condition={data.metadata?.condition as string} {...props} />;
  }
}
