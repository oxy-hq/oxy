import useAgent from "@/hooks/api/useAgent";
import TestSkeleton from "./TestSkeleton";
import TestItem from "./TestItem";
import EmptyState from "@/components/ui/EmptyState";

interface AgentTestsProps {
  agentPathb64: string;
}

const AgentTests = ({ agentPathb64 }: AgentTestsProps) => {
  const { data: agent, isLoading } = useAgent(agentPathb64);

  const tests = agent?.tests || [];
  const shouldShowTests = !isLoading && tests.length > 0;

  return (
    <div className="flex flex-col h-full overflow-auto customScrollbar px-4 pb-4">
      <div className="w-full flex flex-col gap-4">
        {isLoading && <TestSkeleton />}
        {shouldShowTests && (
          <div className="flex flex-col gap-4">
            {tests.map((test, index) => (
              <TestItem
                key={index}
                test={test}
                agentPathb64={agentPathb64}
                index={index}
              />
            ))}
          </div>
        )}

        {tests.length === 0 && (
          <EmptyState
            className="h-full mt-[150px]"
            title="No tests yet"
            description="Create a test to get started"
          />
        )}
      </div>
    </div>
  );
};

export default AgentTests;
