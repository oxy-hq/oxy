import { TaskData } from ".";
import { AgentTask } from "./AgentTask";
import { FormatterTask } from "./FormatterTask";
import { ExecuteSqlTask } from "./ExecuteSqlTask";
import { LoopSequentialTask } from "./LoopSequentialTask";

type Props = {
  task: TaskData;
};

export function TaskItem({ task }: Props) {
  if (task.type === "loop_sequential") {
    return <LoopSequentialTask task={task} />;
  }
  if (task.type === "execute_sql") {
    return <ExecuteSqlTask task={task} />;
  }
  if (task.type === "formatter") {
    return <FormatterTask task={task} />;
  }
  if (task.type === "agent") {
    return <AgentTask task={task} />;
  }
}
