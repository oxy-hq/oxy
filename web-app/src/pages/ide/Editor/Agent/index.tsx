import { useState } from "react";
import EditorPageWrapper from "../components/EditorPageWrapper";
import AgentPreview from "./Preview";
import { randomKey } from "@/libs/utils/string";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";
import AgentTests from "./Tests";
import { Button } from "@/components/ui/shadcn/button";
import { BrushCleaning, Play } from "lucide-react";
import useAgent from "@/hooks/api/agents/useAgent";
import useTests from "@/stores/useTests";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import useAgentThreadStore from "@/stores/useAgentThread";

const AgentEditor = ({ pathb64 }: { pathb64: string }) => {
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  const [selected, setSelected] = useState<string>("preview");
  const queryClient = useQueryClient();
  const { setMessages } = useAgentThreadStore();

  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

  const handleRunAllTests = () => {
    if (isLoading) return;
    const tests = agent?.tests || [];
    for (const [index] of tests.entries()) {
      runTest(pathb64, index);
    }
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={() => {
        setPreviewKey(randomKey());
        queryClient.invalidateQueries({ queryKey: queryKeys.agent.list() });
      }}
      pageContentClassName="md:flex-row flex-col"
      editorClassName="md:w-1/2 w-full h-1/2 md:h-full"
      preview={
        <div className="flex-1 overflow-hidden flex flex-col">
          <div className="flex justify-between p-4">
            <ToggleGroup
              size="sm"
              value={selected}
              onValueChange={setSelected}
              type="single"
            >
              <ToggleGroupItem value="preview" aria-label="Preview">
                Preview
              </ToggleGroupItem>
              <ToggleGroupItem value="test" aria-label="Test">
                Test
              </ToggleGroupItem>
            </ToggleGroup>
            {selected === "test" && (
              <Button size="sm" variant="ghost" onClick={handleRunAllTests}>
                <Play className="w-4 h-4" />
                Run all tests
              </Button>
            )}
            {selected === "preview" && (
              <Button
                size="sm"
                variant={"ghost"}
                onClick={() => {
                  setMessages(pathb64, []);
                }}
              >
                <BrushCleaning className="w-4 h-4" />
                Clean
              </Button>
            )}
          </div>

          <div className="flex-1 overflow-hidden">
            {selected === "preview" ? (
              <AgentPreview key={previewKey} agentPathb64={pathb64 ?? ""} />
            ) : (
              <AgentTests key={previewKey} agentPathb64={pathb64 ?? ""} />
            )}
          </div>
        </div>
      }
    />
  );
};
export default AgentEditor;
