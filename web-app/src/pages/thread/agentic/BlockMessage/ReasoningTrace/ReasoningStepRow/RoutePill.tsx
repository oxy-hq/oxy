import { GitBranch } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { PILL_CLASS } from "./helpers";

interface RoutePillProps {
  name: string;
  onClick: () => void;
}

const RoutePill = ({ name, onClick }: RoutePillProps) => (
  <button
    type='button'
    onClick={(e) => {
      e.stopPropagation();
      onClick();
    }}
    className={cn(PILL_CLASS, "max-w-[160px]")}
  >
    <GitBranch className='h-3 w-3 shrink-0' />
    <span className='truncate'>{name}</span>
  </button>
);

export default RoutePill;
