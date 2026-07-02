import { Check, Copy, KeyRound, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import type { PublishToken } from "@/types/publishTokens";

function formatDate(value: string | null): string {
  return value ? new Date(value).toLocaleString() : "Never";
}

interface PublishTokenRowProps {
  token: PublishToken;
  onRevoke: (token: PublishToken) => void;
}

/**
 * One token in the list. The full secret is never available here (the server
 * stores only a hash), so we show the recognizable prefix as a masked,
 * copy-the-visible-value chip. Revoking the real secret happens once, in the
 * create dialog.
 */
export function PublishTokenRow({ token, onRevoke }: PublishTokenRowProps) {
  const [copied, setCopied] = useState(false);

  const copyPrefix = async () => {
    try {
      await navigator.clipboard.writeText(token.token_prefix);
      setCopied(true);
      toast.success("Token prefix copied");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Couldn't copy");
    }
  };

  return (
    <li className='flex items-center gap-3 px-4 py-3'>
      <div className='flex size-9 shrink-0 items-center justify-center rounded-md border bg-muted/40'>
        <KeyRound className='size-4 text-muted-foreground' />
      </div>

      <div className='min-w-0 flex-1'>
        <div className='flex items-center gap-2'>
          <span className='truncate font-medium text-sm'>{token.name}</span>
          {token.revoked ? (
            <Badge variant='outline' className='text-muted-foreground'>
              Revoked
            </Badge>
          ) : (
            <Badge variant='secondary'>Active</Badge>
          )}
        </div>

        <div className='mt-1 flex items-center gap-1.5'>
          <code className='select-all rounded bg-muted px-1.5 py-0.5 font-mono text-muted-foreground text-xs'>
            {token.token_prefix}…
          </code>
          <button
            type='button'
            onClick={copyPrefix}
            aria-label='Copy token prefix'
            className='inline-flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
          >
            {copied ? <Check className='size-3.5' /> : <Copy className='size-3.5' />}
          </button>
        </div>

        <p className='mt-1 text-muted-foreground text-xs tabular-nums'>
          Created {formatDate(token.created_at)} · Last used {formatDate(token.last_used_at)}
        </p>
      </div>

      {!token.revoked && (
        <Button
          variant='ghost'
          size='icon'
          onClick={() => onRevoke(token)}
          aria-label={`Revoke ${token.name}`}
        >
          <Trash2 className='size-4 text-muted-foreground' />
        </Button>
      )}
    </li>
  );
}
