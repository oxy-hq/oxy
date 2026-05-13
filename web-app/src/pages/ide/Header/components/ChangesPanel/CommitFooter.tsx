import { Upload } from "lucide-react";
import { useRef, useState } from "react";
import { Spinner } from "@/components/ui/shadcn/spinner";

const DEFAULT_MESSAGE = "Auto-commit: Oxygen changes";

interface Props {
  isPushing: boolean;
  pushLabel: string;
  onPush: (message: string) => void;
  onClose: () => void;
}

export function CommitFooter({ isPushing, pushLabel, onPush, onClose }: Props) {
  const [message, setMessage] = useState(DEFAULT_MESSAGE);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = () => {
    if (!message.trim() || isPushing) return;
    onPush(message.trim());
    onClose();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div className='flex flex-col border-border/40 border-t'>
      <div className='border-border/40 border-b px-3 py-2'>
        <span className='font-mono text-[10px] text-muted-foreground/50 uppercase tracking-widest'>
          commit message
        </span>
      </div>
      <div className='px-3 py-2.5'>
        <textarea
          ref={textareaRef}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={handleKeyDown}
          rows={2}
          placeholder='Describe your changes…'
          data-testid='changes-panel-commit-message'
          className='w-full resize-none bg-transparent font-mono text-foreground text-sm placeholder:text-muted-foreground/30 focus:outline-none'
        />
      </div>
      <div className='flex items-center justify-between border-border/40 border-t px-3 py-2'>
        <span className='font-mono text-[10px] text-muted-foreground/35'>⌘↵ to push</span>
        <button
          type='button'
          onClick={handleSubmit}
          disabled={!message.trim() || isPushing}
          data-testid='changes-panel-push-button'
          className='flex items-center gap-1.5 rounded bg-gradient-to-b from-[var(--blue-500)] to-[var(--blue-600)] px-3 py-1 font-medium text-white text-xs shadow-[var(--blue-900)]/40 shadow-sm transition-all hover:from-[var(--blue-400)] hover:to-[var(--blue-500)] disabled:opacity-50'
        >
          {isPushing ? <Spinner className='size-3' /> : <Upload className='h-3 w-3' />}
          {pushLabel}
        </button>
      </div>
    </div>
  );
}
