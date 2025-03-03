import { AgentStep } from "./AgentStep";
import { FormatterStep } from "./FormatterStep";
import { ExecuteSqlStep } from "./ExecuteSqlStep";
import { LoopSequentialStep } from "./LoopSequentialStep";
import { TaskConfigWithId } from "@/stores/useWorkflow";

type Props = {
  task: TaskConfigWithId;
};

export function StepItem({ task }: Props) {
  if (task.type === "loop_sequential") {
    return <LoopSequentialStep task={task} />;
  }
  if (task.type === "execute_sql") {
    return <ExecuteSqlStep task={task} />;
  }
  if (task.type === "formatter") {
    return <FormatterStep task={task} />;
  }
  if (task.type === "agent") {
    return <AgentStep task={task} />;
  }
}
