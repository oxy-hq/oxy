import type { LucideIcon } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface PageHeaderProps {
  icon: LucideIcon;
  title: string;
  description: string;
  actions?: React.ReactNode;
  className?: string;
}

export default function PageHeader({
  icon: Icon,
  title,
  description,
  actions,
  className
}: PageHeaderProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between border-b bg-background/95 p-4 backdrop-blur supports-[backdrop-filter]:bg-background/60",
        className
      )}
    >
      <div className='flex items-center gap-3'>
        <div className='rounded-lg bg-primary/10 p-2'>
          <Icon className='h-5 w-5 text-primary' />
        </div>
        <div>
          <h1 className='font-semibold text-xl'>{title}</h1>
          <p className='text-muted-foreground text-sm'>{description}</p>
        </div>
      </div>
      {actions && <div className='flex items-center gap-3'>{actions}</div>}
    </div>
  );
}
