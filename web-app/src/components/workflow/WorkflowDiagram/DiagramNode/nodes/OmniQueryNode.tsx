import { TaskConfigWithId } from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";

type Props = {
  task: TaskConfigWithId;
};

export function OmniQueryNode({ task }: Props) {
  return <NodeHeader name={task.name} type={task.type} />;
}
