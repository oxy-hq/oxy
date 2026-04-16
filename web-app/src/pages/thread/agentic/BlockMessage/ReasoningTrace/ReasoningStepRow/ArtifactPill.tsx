import { BadgeCheck, BarChart3, Table2 } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { cn } from "@/libs/shadcn/utils";
import { VERIFIED_TOOLTIP } from "@/pages/thread/constants";
import type { Block } from "@/services/types";
import { PILL_CLASS } from "./helpers";

interface ArtifactPillProps {
  block: Block;
  label: string;
  onClick: () => void;
}

const ArtifactPill = ({ block, label, onClick }: ArtifactPillProps) => {
  const Icon = block.type === "viz" ? BarChart3 : Table2;
  const verified = block.type === "semantic_query";
  const pill = (
    <button
      type='button'
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(PILL_CLASS, "max-w-[120px]")}
    >
      <Icon className='h-3 w-3 shrink-0' />
      <span className='truncate'>{label}</span>
      {verified && <BadgeCheck className='h-3 w-3 shrink-0 text-primary' />}
    </button>
  );
  if (!verified) return pill;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{pill}</TooltipTrigger>
      <TooltipContent side='top'>{VERIFIED_TOOLTIP}</TooltipContent>
    </Tooltip>
  );
};

export default ArtifactPill;
