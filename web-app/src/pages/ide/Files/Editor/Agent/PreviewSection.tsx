import { BrushCleaning, Play } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import useAgent from "@/hooks/api/agents/useAgent";
import useAgentThreadStore from "@/stores/useAgentThread";
import useTests from "@/stores/useTests";
import { useEditorContext } from "../contexts/useEditorContext";
import AgentPreview from "./Preview";
import AgentTests from "./Tests";

interface PreviewSectionProps {
  pathb64: string;
  previewKey: string;
}

const PreviewSection = ({ pathb64, previewKey }: PreviewSectionProps) => {
  const [selected, setSelected] = useState<string>("preview");
  const { setMessages } = useAgentThreadStore();
  const { project, branchName } = useEditorContext();
  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

  const handleRunAllTests = () => {
    if (isLoading) return;
    const tests = agent?.tests || [];
    for (const [index] of tests.entries()) {
      runTest(project.id, branchName, pathb64, index);
    }
  };

  return (
    <div className='flex flex-1 flex-col overflow-hidden'>
      <div className='relative z-10 flex flex-shrink-0 justify-between bg-background p-4'>
        <ToggleGroup size='sm' value={selected} onValueChange={setSelected} type='single'>
          <ToggleGroupItem value='preview' aria-label='Preview'>
            Preview
          </ToggleGroupItem>
          <ToggleGroupItem value='test' aria-label='Test'>
            Test
          </ToggleGroupItem>
        </ToggleGroup>
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
              setMessages(pathb64, []);
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
