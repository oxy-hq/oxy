import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import useAgent from "@/hooks/api/useAgent";
import { CirclePlay } from "lucide-react";
import { useParams } from "react-router-dom";
import TestSkeleton from "./TestSkeleton";
import Header from "./Header";
import TestItem from "./TestItem";
import useTests from "@/stores/useTests";

const AgentTestsPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

  const tests = agent?.tests || [];
  const shouldShowTests = !isLoading && tests.length > 0;

  const handleRunAllTests = () => {
    for (const [index] of tests.entries()) {
      runTest(pathb64, index);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <Header />

      <div className="overflow-y-auto px-8 py-10 customScrollbar">
        <div className="max-w-[1100px] w-full mx-auto flex flex-col gap-4">
          <div className="flex w-full gap-3 items-center justify-between">
            <Label className="font-medium text-lg">Test cases</Label>
            <Button onClick={handleRunAllTests}>
              <CirclePlay className="w-4 h-4" />
              Run all
            </Button>
          </div>
          {isLoading && <TestSkeleton />}
          {shouldShowTests && (
            <div className="flex flex-col gap-4">
              {tests.map((test, index) => (
                <TestItem
                  key={index}
                  test={test}
                  agentPathb64={pathb64}
                  index={index}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default AgentTestsPage;
