import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
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

const ArtifactSidebar = ({ block, onClose, onRerun }: ArtifactSidebarProps) => (
  <Panel>
    <PanelHeader title={getSidebarTitle(block)} onClose={onClose} />
    <PanelContent scrollable={false} padding={false} className='min-h-0'>
      <ArtifactBlockRenderer block={block} onRerun={onRerun} />
    </PanelContent>
  </Panel>
);

export default ArtifactSidebar;
