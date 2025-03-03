import { TaskData } from ".";
import { TaskContainer } from "./TaskContainer";
import { TaskHeader } from "./TaskHeader";

type Props = {
  task: TaskData;
};

export function AgentTask({ task }: Props) {
  return (
    <TaskContainer>
      <TaskHeader task={task} />
    </TaskContainer>
  );
}
