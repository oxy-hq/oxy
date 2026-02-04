import EmptyState from "@/components/ui/EmptyState";
import useAgent from "@/hooks/api/agents/useAgent";
import TestItem from "./TestItem";
import TestSkeleton from "./TestSkeleton";

interface AgentTestsProps {
  agentPathb64: string;
}

const AgentTests = ({ agentPathb64 }: AgentTestsProps) => {
  const { data: agent, isLoading } = useAgent(agentPathb64);

  const tests = agent?.tests || [];
  const shouldShowTests = !isLoading && tests.length > 0;

  return (
    <div className='customScrollbar flex h-full flex-col overflow-auto px-4 pb-4'>
      <div className='flex w-full flex-col gap-4'>
        {isLoading && <TestSkeleton />}
        {shouldShowTests && (
          <div className='flex flex-col gap-4'>
            {tests.map((test, index) => (
              <TestItem key={index} test={test} agentPathb64={agentPathb64} index={index} />
            ))}
          </div>
        )}

        {tests.length === 0 && (
          <EmptyState
            className='mt-[150px] h-full'
            title='No tests yet'
            description='Create a test to get started'
          />
        )}
      </div>
    </div>
  );
};

export default AgentTests;
