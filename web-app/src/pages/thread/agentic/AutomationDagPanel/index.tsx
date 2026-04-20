import { BarChart3, Bot, CodeXml, FileText, GitBranch, Globe, Pencil } from "lucide-react";
import type { ElementType } from "react";
import { useNavigate } from "react-router-dom";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { TaskType } from "@/stores/useWorkflow";
import type { AutomationGenerated } from "../BlockMessage";

const NODE_ICONS: Record<string, ElementType> = {
  [TaskType.SEMANTIC_QUERY]: Globe,
  [TaskType.EXECUTE_SQL]: CodeXml,
  [TaskType.AGENT]: Bot,
  [TaskType.FORMATTER]: FileText,
  [TaskType.VISUALIZE]: BarChart3,
  [TaskType.WORKFLOW]: GitBranch
};

const NODE_LABELS: Record<string, string> = {
  [TaskType.SEMANTIC_QUERY]: "Semantic Query",
  [TaskType.EXECUTE_SQL]: "Execute SQL",
  [TaskType.AGENT]: "Agent",
  [TaskType.FORMATTER]: "Formatter",
  [TaskType.VISUALIZE]: "Visualize",
  [TaskType.WORKFLOW]: "Sub-procedure"
};

interface AutomationDagPanelProps {
  automationGenerated: AutomationGenerated;
  highlightedNodeId: string | null;
  onNodeHover: (nodeId: string | null) => void;
  onClose: () => void;
}

const AutomationDagPanel = ({
  automationGenerated,
  highlightedNodeId,
  onNodeHover,
  onClose
}: AutomationDagPanelProps) => {
  const { tasks } = automationGenerated;
  const { project } = useCurrentProjectBranch();
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  return (
    <Panel>
      <PanelHeader
        title='Generated Procedure'
        onClose={onClose}
        actions={
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => {
              navigate(
                ROUTES.ORG(orgSlug)
                  .WORKSPACE(project.id)
                  .IDE.FILES.FILE(encodeBase64(automationGenerated.path))
              );
            }}
          >
            <Pencil className='h-4 w-4' />
          </Button>
        }
      />

      {/* Node list */}
      <PanelContent scrollable={false} padding={false} className='overflow-y-auto p-4'>
        <div className='flex flex-col items-center gap-0'>
          {tasks.map((task, i) => {
            const Icon = NODE_ICONS[task.type] ?? Globe;
            const label = NODE_LABELS[task.type] ?? task.type;
            const isHighlighted = highlightedNodeId === task.name;
            const dimmed = highlightedNodeId !== null && !isHighlighted;

            return (
              <div key={task.name} className='flex flex-col items-center'>
                {i > 0 && (
                  <div
                    className={cn(
                      "h-6 w-px transition-colors duration-300",
                      isHighlighted ? "bg-primary" : "bg-border"
                    )}
                  />
                )}
                <div
                  onMouseEnter={() => onNodeHover(task.name)}
                  onMouseLeave={() => onNodeHover(null)}
                  className={cn(
                    "relative flex cursor-default items-center gap-2.5 rounded-lg border px-3 py-2.5 transition-all duration-300",
                    isHighlighted
                      ? "border-primary bg-primary/10 shadow-[0_0_12px_color-mix(in_srgb,var(--blue-500)_15%,transparent)]"
                      : dimmed
                        ? "border-border/50 bg-card/30 opacity-40"
                        : "border-border bg-card"
                  )}
                >
                  <div className='flex h-7 w-7 items-center justify-center rounded-md bg-secondary'>
                    <Icon
                      className={cn(
                        "h-3.5 w-3.5",
                        dimmed ? "text-muted-foreground" : "text-foreground"
                      )}
                    />
                  </div>
                  <div className='min-w-0'>
                    <div
                      className={cn(
                        "font-medium text-xs",
                        dimmed ? "text-muted-foreground" : "text-foreground"
                      )}
                    >
                      {label}
                    </div>
                    <div className='max-w-[120px] truncate font-mono text-[10px] text-muted-foreground'>
                      {task.name}
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </PanelContent>
    </Panel>
  );
};

export default AutomationDagPanel;
