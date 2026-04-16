import { BadgeCheck } from "lucide-react";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { VERIFIED_TOOLTIP } from "@/pages/thread/constants";
import type { Block } from "@/services/types";
import ArtifactBlockRenderer from "./ArtifactBlockRenderer";

const getSidebarTitle = (block: Block): string => {
  if (block.type === "group") return "Procedure trace";
  if (block.type === "semantic_query") return "Semantic Query";
  if (block.type === "looker_query") return "Looker Query";
  if (block.type === "sql") return "SQL Query";
  if (block.type === "text" && block.content?.startsWith("Selected route:"))
    return "Selected route";
  if (block.type === "data_app") return "Data app";
  if (block.type === "viz") return "Visualization";
  return `${block.type} artifact`;
};

interface ArtifactSidebarProps {
  block: Block;
  onClose: () => void;
  onRerun?: (prompt: string) => void;
}

const ArtifactSidebar = ({ block, onClose, onRerun }: ArtifactSidebarProps) => {
  const verified = block.type === "semantic_query";
  return (
    <Panel>
      <PanelHeader
        title={
          <div className='flex min-w-0 items-center gap-1.5'>
            <h3 className='truncate font-semibold text-sm'>{getSidebarTitle(block)}</h3>
            {verified && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <BadgeCheck className='h-4 w-4 shrink-0 text-primary' />
                </TooltipTrigger>
                <TooltipContent side='bottom'>{VERIFIED_TOOLTIP}</TooltipContent>
              </Tooltip>
            )}
          </div>
        }
        onClose={onClose}
      />
      <PanelContent scrollable={false} padding={false} className='min-h-0'>
        <ArtifactBlockRenderer block={block} onRerun={onRerun} />
      </PanelContent>
    </Panel>
  );
};

export default ArtifactSidebar;
