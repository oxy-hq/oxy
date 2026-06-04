import type { ReactNode } from "react";
import { cn } from "@/libs/utils/cn";

interface Props {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}

/**
 * A simple titled card wrapper used across the internal jobs page.
 * Kept ultra-thin so each section reads top-to-bottom.
 */
export function SectionCard({ title, description, action, children, className }: Props) {
  return (
    <section className={cn("rounded-lg border border-border bg-card", className)}>
      <header className='flex items-start justify-between gap-4 border-border border-b p-4'>
        <div>
          <h2 className='font-semibold text-base text-foreground'>{title}</h2>
          {description ? (
            <p className='mt-0.5 text-muted-foreground text-sm'>{description}</p>
          ) : null}
        </div>
        {action ? <div className='shrink-0'>{action}</div> : null}
      </header>
      <div className='p-4'>{children}</div>
    </section>
  );
}
