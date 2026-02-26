import { Fullscreen } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { Block } from "@/services/types";
import BlockComponent, { isFullscreenableBlock } from "./BlockComponent";

interface BlockContentProps {
  block: Block;
  onFullscreen?: (blockId: string) => void;
}

const BlockContent = ({ block, onFullscreen }: BlockContentProps) => (
  <div className='group relative'>
    <BlockComponent block={block} />
    {!!onFullscreen && isFullscreenableBlock(block) && (
      <Button
        variant='ghost'
        size='icon'
        className='absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100'
        onClick={() => onFullscreen(block.id)}
      >
        <Fullscreen size={16} />
      </Button>
    )}
  </div>
);

export default BlockContent;
