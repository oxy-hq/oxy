import { BrushCleaning, FlaskConical, MessageCircleDashed, Play } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import useAgent from "@/hooks/api/agents/useAgent";
import useAgentThreadStore, { getThreadIdFromPath } from "@/stores/useAgentThread";
import useTests from "@/stores/useTests";
import { useEditorContext } from "../contexts/useEditorContext";
import AgentPreview from "./Preview";
import AgentTests from "./Tests";

interface PreviewSectionProps {
  pathb64: string;
  previewKey: string;
}

const PreviewSection = ({ pathb64, previewKey }: PreviewSectionProps) => {
  const [selected, setSelected] = useState("preview");
  const { setMessages } = useAgentThreadStore();
  const { project, branchName } = useEditorContext();
  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

  const threadId = getThreadIdFromPath(project.id, branchName, pathb64);

  const handleRunAllTests = () => {
    if (isLoading) return;
    const tests = agent?.tests || [];
    for (const [index] of tests.entries()) {
      runTest(project.id, branchName, pathb64, index);
    }
  };

  return (
    <div className='flex flex-1 flex-col overflow-hidden'>
      <div className='relative z-10 flex flex-shrink-0 justify-between bg-background p-2'>
        <Tabs value={selected} onValueChange={setSelected}>
          <TabsList>
            <TabsTrigger value='preview'>
              <MessageCircleDashed className='h-4 w-4' />
              Preview
            </TabsTrigger>
            <TabsTrigger value='test'>
              <FlaskConical />
              Test
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {selected === "test" && (
          <Button size='sm' variant='ghost' onClick={handleRunAllTests} title={"Run all tests"}>
            <Play className='h-4 w-4' />
            Run all tests
          </Button>
        )}
        {selected === "preview" && (
          <Button
            size='sm'
            variant={"ghost"}
            onClick={() => {
              setMessages(threadId, []);
            }}
          >
            <BrushCleaning className='h-4 w-4' />
            Clean
          </Button>
        )}
      </div>

      <div className='flex-1 overflow-auto'>
        {selected === "preview" ? (
          <AgentPreview key={previewKey} agentPathb64={pathb64} />
        ) : (
          <AgentTests key={previewKey} agentPathb64={pathb64} />
        )}
      </div>
    </div>
  );
};

export default PreviewSection;
