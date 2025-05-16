import { AgentNode } from "./nodes/AgentNode";
import { FormatterNode } from "./nodes/FormatterNode";
import { ExecuteSqlNode } from "./nodes/ExecuteSqlNode";
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

type Props = {
  task: TaskConfigWithId;
  type: NodeType;
  data: NodeData;
  width?: number;
  height?: number;
};

export function NodeContent({ task, type, data, ...props }: Props) {
  if (task.type === "loop_sequential") {
    return <LoopSequentialNode task={task} />;
  }
  if (task.type === "execute_sql") {
    return <ExecuteSqlNode task={task} />;
  }
  if (task.type === "formatter") {
    return <FormatterNode task={task} />;
  }
  if (task.type === "agent") {
    return <AgentNode task={task} />;
  }

  if (task.type === "workflow") {
    return <WorkflowTaskNode task={task} />;
  }

  if (type === TaskType.CONDITIONAL) {
    return <ConditionalNode task={task} />;
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
