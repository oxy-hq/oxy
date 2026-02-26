import { BarChart3, Table2 } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { Block } from "@/services/types";
import { PILL_CLASS } from "./helpers";

interface ArtifactPillProps {
  block: Block;
  label: string;
  onClick: () => void;
}

const ArtifactPill = ({ block, label, onClick }: ArtifactPillProps) => {
  const Icon = block.type === "viz" ? BarChart3 : Table2;
  return (
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
    </button>
  );
};

export default ArtifactPill;
