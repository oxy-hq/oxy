import { useState } from "react";
import EditorPageWrapper from "../components/EditorPageWrapper";
import AgentPreview from "./Preview";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";
import AgentTests from "./Tests";
import { Button } from "@/components/ui/shadcn/button";
import { BrushCleaning, Play } from "lucide-react";
import useAgent from "@/hooks/api/agents/useAgent";
import useTests from "@/stores/useTests";
import useAgentThreadStore from "@/stores/useAgentThread";
import { useEditorContext } from "../contexts/useEditorContext";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { usePreviewRefresh } from "../usePreviewRefresh";

const AgentEditor = () => {
  const { pathb64, project, branchName, isReadOnly, gitEnabled } =
    useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const [selected, setSelected] = useState<string>("preview");
  const { setMessages } = useAgentThreadStore();
  const { invalidateAgentQueries } = useEditorQueryInvalidation();

  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

  const handleSaved = () => {
    refreshPreview();
    invalidateAgentQueries();
  };

  const handleRunAllTests = () => {
    if (isLoading) return;
    const tests = agent?.tests || [];
    for (const [index] of tests.entries()) {
      runTest(project.id, branchName, pathb64, index);
    }
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      preview={
        <div className="flex-1 overflow-hidden flex flex-col">
          <div className="flex justify-between p-4 flex-shrink-0 relative z-10 bg-background">
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
              <Button
                size="sm"
                variant="ghost"
                onClick={handleRunAllTests}
                title={"Run all tests"}
              >
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

          <div className="flex-1 overflow-auto">
            {selected === "preview" ? (
              <AgentPreview key={previewKey} agentPathb64={pathb64} />
            ) : (
              <AgentTests key={previewKey} agentPathb64={pathb64} />
            )}
          </div>
        </div>
      }
    />
  );
};
export default AgentEditor;
