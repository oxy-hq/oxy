import type { LucideIcon } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface PageHeaderProps {
  icon: LucideIcon;
  title: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export default function PageHeader({ icon: Icon, title, actions, className }: PageHeaderProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between border-b bg-background/95 px-4 py-1 backdrop-blur supports-[backdrop-filter]:bg-background/60",
        className
      )}
    >
      <div className='flex min-h-8 items-center gap-3'>
        <Icon className='h-4 w-4 text-primary' />
        <h1 className='font-semibold text-sm'>{title}</h1>
      </div>
      {actions && <div className='flex items-center gap-3'>{actions}</div>}
    </div>
  );
}
