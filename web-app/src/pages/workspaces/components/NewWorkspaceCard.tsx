import { Plus } from "lucide-react";

interface Props {
  index: number;
  onClick: () => void;
}

export function NewWorkspaceCard({ index, onClick }: Props) {
  return (
    <li
      className='fade-in slide-in-from-bottom-2 h-full animate-in fill-mode-both duration-300'
      style={{ animationDelay: `${index * 60}ms` }}
    >
      <button
        type='button'
        onClick={onClick}
        className='group flex h-full min-h-[130px] w-full flex-col items-center justify-center gap-2.5 rounded-xl border border-border/50 border-dashed bg-card transition-all hover:border-primary/40 hover:bg-primary/[0.02]'
      >
        <div className='flex h-8 w-8 items-center justify-center rounded-full border border-border transition-all group-hover:border-primary/40 group-hover:bg-primary/10'>
          <Plus className='h-4 w-4 text-muted-foreground/60 transition-colors group-hover:text-primary' />
        </div>
        <span className='font-medium text-muted-foreground/60 text-sm transition-colors group-hover:text-primary'>
          New workspace
        </span>
      </button>
    </li>
  );
}
