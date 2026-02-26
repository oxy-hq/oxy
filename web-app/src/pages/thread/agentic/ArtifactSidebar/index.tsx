import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { Block } from "@/services/types";
import ArtifactBlockRenderer from "./ArtifactBlockRenderer";

const getSidebarTitle = (block: Block): string => {
  if (block.type === "group") return "Automation trace";
  if (block.type === "semantic_query") return "Semantic Query";
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
  <div className='flex h-full flex-col bg-card/50'>
    <div className='flex shrink-0 items-center justify-between border-border border-b px-3 py-2'>
      <span className='truncate font-medium text-muted-foreground text-sm'>
        {getSidebarTitle(block)}
      </span>
      <Button variant='ghost' size='icon' className='h-6 w-6 shrink-0' onClick={onClose}>
        <X className='h-3.5 w-3.5' />
      </Button>
    </div>
    <div className='min-h-0 flex-1'>
      <ArtifactBlockRenderer block={block} onRerun={onRerun} />
    </div>
  </div>
);

export default ArtifactSidebar;
