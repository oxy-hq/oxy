import { TaskData } from ".";
import { TaskContainer } from "./TaskContainer";
import { TaskHeader } from "./TaskHeader";

type Props = {
  task: TaskData;
};

export function ExecuteSqlTask({ task }: Props) {
  return (
    <TaskContainer>
      <TaskHeader task={task} />
    </TaskContainer>
  );
}
