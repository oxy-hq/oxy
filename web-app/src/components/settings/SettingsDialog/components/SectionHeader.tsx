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
    <div
      className={cn(
        "flex flex-col gap-3 border-b pb-4 sm:flex-row sm:items-end sm:justify-between sm:gap-4",
        className
      )}
    >
      <div className='flex min-w-0 flex-col gap-1'>
        <h3 className='flex items-center gap-2 font-semibold text-base'>
          {Icon && <Icon className='h-4 w-4 shrink-0 text-muted-foreground' />}
          <span className='min-w-0 break-words'>{title}</span>
        </h3>
        {description && <p className='text-muted-foreground text-xs'>{description}</p>}
      </div>
      {actions && <div className='flex flex-wrap items-center gap-2 sm:shrink-0'>{actions}</div>}
    </div>
  );
}
