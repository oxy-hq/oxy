import type { TaskConfigWithId } from "@/stores/useAutomation";
import { NodeHeader } from "./NodeHeader";

type Props = {
  task: TaskConfigWithId;
};

export function OmniQueryNode({ task }: Props) {
  return <NodeHeader name={task.name} type={task.type} />;
}
