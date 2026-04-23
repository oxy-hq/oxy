import type React from "react";

interface Props {
  label: string;
  icon: React.ReactNode;
  title: string;
  description: string;
  badge?: string;
  onClick: () => void;
}

export function OptionCard({ label, icon, title, description, badge, onClick }: Props) {
  return (
    <button
      type='button'
      onClick={onClick}
      className='group flex items-center gap-4 rounded-lg border border-border bg-transparent px-4 py-3.5 text-left transition-all hover:border-primary/40 hover:bg-primary/[0.03]'
    >
      <span className='shrink-0 font-mono text-[11px] text-muted-foreground/50 tabular-nums'>
        {label}
      </span>
      <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <div className='flex items-center gap-2'>
          <span className='font-medium text-foreground text-sm transition-colors group-hover:text-primary'>
            {title}
          </span>
          {badge && (
            <span className='rounded-full bg-primary/10 px-2 py-0.5 font-medium text-[10px] text-primary'>
              {badge}
            </span>
          )}
        </div>
        <span className='text-muted-foreground text-xs leading-relaxed'>{description}</span>
      </div>
      <span className='shrink-0 text-muted-foreground/30 transition-colors group-hover:text-primary/60'>
        {icon}
      </span>
    </button>
  );
}
