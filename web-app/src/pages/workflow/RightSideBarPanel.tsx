import useWorkflow, { TaskType } from "@/stores/useWorkflow";
import ExecuteSqlSidebar from "./ExecuteSqlSidebar";
import { FormatterSidebar } from "./FormatterSidebar";
import { AgentSidebar } from "./AgentSidebar";
import { LoopSequentialSidebar } from "./LoopSequentialSidebar";

type Props = {
  nodeId: string;
};

const RightSideBarPanel = ({ nodeId }: Props) => {
  const node = useWorkflow((state) => state.getNode(nodeId));
  if (!node) {
    return null;
  }
  if (node.type === TaskType.EXECUTE_SQL) {
    return <ExecuteSqlSidebar node={node} key={nodeId} />;
  }
  if (node.type === TaskType.FORMATTER) {
    return <FormatterSidebar node={node} key={nodeId} />;
  }

  if (node.type === TaskType.AGENT) {
    return <AgentSidebar node={node} key={nodeId} />;
  }

  if (node.type === TaskType.LOOP_SEQUENTIAL) {
    return <LoopSequentialSidebar node={node} key={nodeId} />;
  }
};

export default RightSideBarPanel;
