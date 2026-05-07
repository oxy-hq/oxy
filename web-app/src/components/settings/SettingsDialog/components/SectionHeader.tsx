import type { LucideIcon } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface SectionHeaderProps {
  icon?: LucideIcon;
  title: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export default function SectionHeader({
  icon: Icon,
  title,
  description,
  actions,
  className
}: SectionHeaderProps) {
  return (
    <div className={cn("flex items-end justify-between gap-4 border-b pb-4", className)}>
      <div className='flex flex-col gap-1'>
        <h3 className='flex items-center gap-2 font-semibold text-base'>
          {Icon && <Icon className='h-4 w-4 text-muted-foreground' />}
          {title}
        </h3>
        {description && <p className='text-muted-foreground text-xs'>{description}</p>}
      </div>
      {actions && <div className='flex shrink-0 items-center gap-2'>{actions}</div>}
    </div>
  );
}
