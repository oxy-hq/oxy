import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { cn } from "@/libs/shadcn/utils";

interface CollapsibleSectionProps {
  title: string;
  count?: number;
  defaultOpen?: boolean;
  className?: string;
  children: React.ReactNode;
}

const CollapsibleSection = ({
  title,
  count,
  defaultOpen = true,
  className,
  children
}: CollapsibleSectionProps) => {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className={cn("border-border border-b", className)}>
      <button
        type='button'
        className='flex w-full items-center gap-1.5 px-3 py-2 font-medium text-muted-foreground text-sm hover:bg-muted/50'
        onClick={() => setOpen((v) => !v)}
      >
        {open ? <ChevronDown className='h-3.5 w-3.5' /> : <ChevronRight className='h-3.5 w-3.5' />}
        {title}
        {count !== undefined && <span className='text-muted-foreground/70 text-xs'>({count})</span>}
      </button>
      <div
        className='overflow-hidden transition-[max-height] duration-200'
        style={{ maxHeight: open ? "2000px" : "0px" }}
      >
        <div className='px-3 pb-3'>{children}</div>
      </div>
    </div>
  );
};

export default CollapsibleSection;
