import { AgentNode } from "./nodes/AgentNode";
import { FormatterNode } from "./nodes/FormatterNode";
import { ExecuteSqlNode } from "./nodes/ExecuteSqlNode";
import { SemanticQueryNode } from "./nodes/SemanticQueryNode";
import { OmniQueryNode } from "./nodes/OmniQueryNode";
import { LoopSequentialNode } from "./nodes/LoopSequentialNode";
import {
  NodeData,
  NodeType,
  NoneTaskNodeType,
  TaskConfigWithId,
  TaskType,
} from "@/stores/useWorkflow";
import { WorkflowTaskNode } from "./nodes/WorkflowTaskNode";
import ConditionalNode from "./nodes/ConditionalNode";
import { ConditionalIfNode } from "./nodes/ConditionalIfNode";
import { ConditionalElseNode } from "./nodes/ConditionalElseNode";
import { TaskRun } from "@/services/types";

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

export function NodeContent({
  parentId,
  task,
  type,
  data,
  taskRun,
  loopRuns,
  ...props
}: Props) {
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
    return (
      <WorkflowTaskNode
        task={task}
        taskRun={taskRun}
        expanded={data.expanded}
      />
    );
  }

  if (type === TaskType.CONDITIONAL) {
    return <ConditionalNode task={task} expanded={data.expanded} />;
  }
  if (type === NoneTaskNodeType.CONDITIONAL_ELSE) {
    return <ConditionalElseNode {...props} />;
  }
  if (type === NoneTaskNodeType.CONDITIONAL_IF) {
    return (
      <ConditionalIfNode
        condition={data.metadata?.condition as string}
        {...props}
      />
    );
  }
}
