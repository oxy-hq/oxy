import { ChevronsRight, Terminal } from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import useModelingNodes from "@/hooks/api/modeling/useModelingNodes";
import useModelingRunStream from "@/hooks/api/modeling/useModelingRunStream";
import useBuilderDialog from "@/stores/useBuilderDialog";

import ModelingActions from "./components/ModelingActions";
import NodeDetail from "./components/NodeDetail";
import NodesList from "./components/NodesList";
import OutputPanel, { type OutputState } from "./components/OutputPanel";
import RunGraph from "./components/RunGraph";

const ModelingPage: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [selectedProjectName, setSelectedProjectName] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [output, setOutput] = useState<OutputState>(null);
  const [outputPending, setOutputPending] = useState(false);
  const [outputOpen, setOutputOpen] = useState(false);
  const [lastOutputKind, setLastOutputKind] = useState<string | null>(null);
  const [runNodeId, setRunNodeId] = useState<string | null>(null);
  const [runProjectName, setRunProjectName] = useState<string | null>(null);

  const { data: nodes } = useModelingNodes(selectedProjectName ?? "");
  const selectedNode = nodes?.find((n) => n.unique_id === selectedNodeId) ?? null;

  const { setModelingSelection } = useBuilderDialog();

  useEffect(() => {
    if (selectedProjectName) {
      setModelingSelection({ projectName: selectedProjectName, node: selectedNode ?? null });
    } else {
      setModelingSelection(null);
    }
  }, [selectedProjectName, selectedNode, setModelingSelection]);

  useEffect(() => {
    return () => setModelingSelection(null);
  }, [setModelingSelection]);

  const runStream = useModelingRunStream(selectedProjectName ?? "");

  const handleSelectProject = useCallback((name: string) => {
    setSelectedProjectName((prev) => (prev === name ? prev : name));
    setSelectedNodeId(null);
  }, []);

  const handleOutput = (next: OutputState) => {
    runStream.reset();
    setOutput(next);
    setOutputOpen(true);
  };

  const handlePendingChange = (pending: boolean) => {
    setOutputPending(pending);
    if (pending) setOutputOpen(true);
  };

  const handleOutputKind = useCallback((kind: string) => {
    setLastOutputKind(kind);
  }, []);

  const handleRunStream = useCallback(
    async (selector?: string) => {
      setRunNodeId(selector ? (selectedNodeId ?? null) : null);
      setRunProjectName(selectedProjectName);
      setLastOutputKind("run");
      setOutputOpen(true);
      await runStream.run(selector);
    },
    [runStream, selectedNodeId, selectedProjectName]
  );

  const kindLabel = lastOutputKind
    ? lastOutputKind.charAt(0).toUpperCase() + lastOutputKind.slice(1)
    : null;

  return (
    <div className='flex h-full flex-1 flex-col overflow-hidden'>
      <ModelingActions
        dbtProjectName={selectedProjectName}
        onOutput={handleOutput}
        onPendingChange={handlePendingChange}
        onRunStream={handleRunStream}
        onOutputKind={handleOutputKind}
        isStreaming={runStream.state.phase === "running"}
      />

      <ResizablePanelGroup direction='vertical' className='flex-1'>
        <ResizablePanel defaultSize={outputOpen ? 60 : 100} minSize={30}>
          <ResizablePanelGroup direction='horizontal'>
            {sidebarOpen ? (
              <>
                <ResizablePanel defaultSize={25} minSize={15} className='min-w-[200px]'>
                  <div className='flex h-full flex-col border-r bg-sidebar-background'>
                    <div className='flex items-center justify-between border-b px-3 py-2'>
                      <span className='font-medium text-sm'>Data Models</span>
                      <Button
                        variant='ghost'
                        size='icon'
                        onClick={() => setSidebarOpen(false)}
                        tooltip={{ content: "Collapse", side: "right" }}
                        className='h-6 w-6'
                      >
                        <ChevronsRight className='h-3.5 w-3.5 rotate-180' />
                      </Button>
                    </div>
                    <NodesList
                      selectedProjectName={selectedProjectName}
                      selectedNodeId={selectedNode?.unique_id ?? null}
                      onSelectProject={handleSelectProject}
                      onSelectNode={(node) => setSelectedNodeId(node.unique_id)}
                    />
                  </div>
                </ResizablePanel>
                <ResizableHandle />
              </>
            ) : (
              <div className='flex items-start border-r bg-sidebar-background px-1 py-2'>
                <Button
                  variant='ghost'
                  size='icon'
                  onClick={() => setSidebarOpen(true)}
                  tooltip={{ content: "Expand Sidebar", side: "right" }}
                  className='h-8 w-8'
                >
                  <ChevronsRight className='h-4 w-4' />
                </Button>
              </div>
            )}
            <ResizablePanel defaultSize={sidebarOpen ? 75 : 100} minSize={30}>
              {selectedNode ? (
                <NodeDetail
                  key={selectedNode.unique_id}
                  node={selectedNode}
                  dbtProjectName={selectedProjectName ?? ""}
                  onRunStream={handleRunStream}
                  isStreaming={runStream.state.phase === "running"}
                />
              ) : (
                <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
                  {selectedProjectName
                    ? "Select a model to view details"
                    : "Select a project to get started"}
                </div>
              )}
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>

        {outputOpen && (
          <>
            <ResizableHandle />
            <ResizablePanel defaultSize={40} minSize={15} maxSize={60}>
              <div className='flex h-full flex-col border-t'>
                <div className='flex items-center justify-between border-b bg-muted/40 px-3 py-1'>
                  <div className='flex items-center gap-1.5 font-medium text-xs'>
                    <Terminal className='h-3.5 w-3.5' />
                    Output{kindLabel ? ` · ${kindLabel}` : ""}
                  </div>
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    onClick={() => setOutputOpen(false)}
                    tooltip={{ content: "Close", side: "top" }}
                  >
                    <ChevronsRight className='h-3.5 w-3.5 rotate-90' />
                  </Button>
                </div>
                <div className='flex-1 overflow-hidden'>
                  {runProjectName && runStream.state.phase !== "idle" ? (
                    <RunGraph
                      dbtProjectName={runProjectName}
                      runStream={runStream.state}
                      selectedNodeId={runNodeId ?? undefined}
                    />
                  ) : (
                    <div className='h-full overflow-y-auto'>
                      <OutputPanel
                        output={output}
                        isPending={outputPending}
                        runStream={runStream.state}
                      />
                    </div>
                  )}
                </div>
              </div>
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>

      {!outputOpen && lastOutputKind && (
        <div className='flex shrink-0 items-center gap-1.5 border-t bg-muted/40 px-3 py-1 font-mono text-xs'>
          <Terminal className='h-3.5 w-3.5' />
          <span className='font-medium'>Output · {kindLabel}</span>
          <Button
            variant='ghost'
            size='icon'
            className='ml-auto h-5 w-5'
            onClick={() => setOutputOpen(true)}
            tooltip={{ content: "Expand", side: "top" }}
          >
            <ChevronsRight className='h-3.5 w-3.5 -rotate-90' />
          </Button>
        </div>
      )}
    </div>
  );
};

export default ModelingPage;
