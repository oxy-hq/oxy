import type { TaskConfigWithId } from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";

type Props = {
  task: TaskConfigWithId;
};

export function AgentNode({ task }: Props) {
  return <NodeHeader type={task.type} name={task.name} />;
}
