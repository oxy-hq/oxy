import { Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";

const DEFAULT_MESSAGE = "Auto-commit: Oxy changes";

interface CommitMessagePopoverProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  trigger: React.ReactNode;
  isPushing: boolean;
  pushLabel: string;
  onPush: (message: string) => void;
}

export const CommitMessagePopover = ({
  open,
  onOpenChange,
  trigger,
  isPushing,
  pushLabel,
  onPush
}: CommitMessagePopoverProps) => {
  const { isLocalMode } = useAuth();
  const [message, setMessage] = useState(DEFAULT_MESSAGE);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-focus and select on open
  useEffect(() => {
    if (open) {
      const t = setTimeout(() => {
        textareaRef.current?.focus();
        textareaRef.current?.select();
      }, 50);
      return () => clearTimeout(t);
    }
  }, [open]);

  if (isLocalMode) return null;

  const handleSubmit = () => {
    if (!message.trim() || isPushing) return;
    onPush(message.trim());
    onOpenChange(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        align='start'
        sideOffset={6}
        className='w-80 overflow-hidden border-border/60 p-0 shadow-black/25 shadow-xl'
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        <div className='flex flex-col'>
          {/* Header row */}
          <div className='border-border/40 border-b px-3 py-2'>
            <span className='font-mono text-[10px] text-muted-foreground/50 uppercase tracking-widest'>
              commit message
            </span>
          </div>

          {/* Textarea */}
          <div className='px-3 py-2.5'>
            <textarea
              ref={textareaRef}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={handleKeyDown}
              rows={3}
              placeholder='Describe your changes…'
              className='w-full resize-none bg-transparent font-mono text-foreground text-sm placeholder:text-muted-foreground/30 focus:outline-none'
            />
          </div>

          {/* Footer */}
          <div className='flex items-center justify-between border-border/40 border-t px-3 py-2'>
            <span className='font-mono text-[10px] text-muted-foreground/35'>⌘↵ to push</span>
            <button
              type='button'
              onClick={handleSubmit}
              disabled={!message.trim() || isPushing}
              className='flex items-center gap-1.5 rounded bg-gradient-to-b from-[var(--blue-500)] to-[var(--blue-600)] px-3 py-1 font-medium text-white text-xs shadow-[var(--blue-900)]/30 shadow-sm transition-all hover:from-[var(--blue-400)] hover:to-[var(--blue-500)] disabled:opacity-50'
            >
              {isPushing ? <Spinner className='size-3' /> : <Upload className='h-3 w-3' />}
              {pushLabel}
            </button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
};
