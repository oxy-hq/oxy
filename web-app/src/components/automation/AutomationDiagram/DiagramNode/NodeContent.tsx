import {
  type NodeData,
  type NodeType,
  NoneTaskNodeType,
  type TaskConfigWithId,
  TaskType
} from "@/stores/useAutomation";
import { AirwayNode } from "./nodes/AirwayNode";
import { AutomationTaskNode } from "./nodes/AutomationTaskNode";
import { ConditionalElseNode } from "./nodes/ConditionalElseNode";
import { ConditionalIfNode } from "./nodes/ConditionalIfNode";
import ConditionalNode from "./nodes/ConditionalNode";
import { ExecuteSqlNode } from "./nodes/ExecuteSqlNode";
import { FormatterNode } from "./nodes/FormatterNode";
import { LookerQueryNode } from "./nodes/LookerQueryNode";
import { LoopSequentialNode } from "./nodes/LoopSequentialNode";
import { OmniQueryNode } from "./nodes/OmniQueryNode";
import { SemanticQueryNode } from "./nodes/SemanticQueryNode";

type Props = {
  id: string;
  task: TaskConfigWithId;
  type: NodeType;
  data: NodeData;
  parentId?: string;
  width?: number;
  height?: number;
};

export function NodeContent({ parentId, task, type, data, ...props }: Props) {
  if (task.type === "loop_sequential") {
    return <LoopSequentialNode parentId={parentId} task={task} expanded={data.expanded} />;
  }
  if (task.type === "execute_sql") {
    return <ExecuteSqlNode task={task} />;
  }
  if (task.type === TaskType.AIRWAY) {
    return <AirwayNode task={task} />;
  }
  if (task.type === TaskType.SEMANTIC_QUERY) {
    return <SemanticQueryNode task={task} />;
  }
  if (task.type === TaskType.LOOKER_QUERY) {
    return <LookerQueryNode task={task} />;
  }
  if (task.type === TaskType.OMNI_QUERY) {
    return <OmniQueryNode task={task} />;
  }
  if (task.type === "formatter") {
    return <FormatterNode task={task} />;
  }

  if (task.type === "workflow") {
    return <AutomationTaskNode task={task} expanded={data.expanded} />;
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
