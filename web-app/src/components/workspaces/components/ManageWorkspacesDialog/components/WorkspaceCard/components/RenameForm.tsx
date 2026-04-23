import { Check, X } from "lucide-react";
import type { RefObject } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Spinner } from "@/components/ui/shadcn/spinner";

type Props = {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
  isPending: boolean;
  error: string | null;
  inputRef: RefObject<HTMLInputElement | null>;
};

export function RenameForm({
  value,
  onChange,
  onSubmit,
  onCancel,
  isPending,
  error,
  inputRef
}: Props) {
  return (
    <div className='flex flex-1 flex-col gap-2 p-5'>
      <p className='font-medium text-muted-foreground text-sm'>Rename workspace</p>
      <div className='flex items-center gap-1.5'>
        <Input
          ref={inputRef}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onSubmit();
            if (e.key === "Escape") onCancel();
          }}
          className='h-7 flex-1 text-sm'
          disabled={isPending}
        />
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-primary hover:bg-primary/10'
          onClick={onSubmit}
          disabled={isPending}
        >
          {isPending ? <Spinner className='size-3' /> : <Check className='size-3' />}
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-muted-foreground hover:bg-muted'
          onClick={onCancel}
          disabled={isPending}
        >
          <X className='size-3' />
        </Button>
      </div>
      {error && <p className='text-destructive text-xs'>{error}</p>}
    </div>
  );
}
