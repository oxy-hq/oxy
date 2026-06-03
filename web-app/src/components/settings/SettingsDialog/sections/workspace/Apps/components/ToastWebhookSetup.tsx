import { Check, ChevronDown, Copy, ExternalLink } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { cn } from "@/libs/shadcn/utils";

/** Best-effort default for the public base URL: where this UI is served from. */
const DEFAULT_BASE_URL = typeof window !== "undefined" ? window.location.origin : "";

const TOAST_WEBHOOKS_URL = "https://www.toasttab.com/restaurants/admin/webhooks";

interface ToastWebhookSetupProps {
  /** The current workspace/project id — used to build the `project_id` query param. */
  projectId: string;
}

/**
 * Collapsible how-to for creating the Toast webhook subscription that feeds
 * this integration. Mirrors the manual steps in `world-model-app/README.md` so
 * the person connecting Toast doesn't have to leave the dialog to find them.
 * Collapsed by default to keep the dialog compact.
 */
export function ToastWebhookSetup({ projectId }: ToastWebhookSetupProps) {
  const [open, setOpen] = useState(false);
  const webhookPath = `/api/webhooks/toast/orders?project_id=${projectId}`;

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className='rounded-md border border-border bg-muted/40'
    >
      <CollapsibleTrigger className='flex w-full items-center justify-between gap-2 rounded-md p-3 text-left hover:bg-muted/60'>
        <span className='font-medium text-sm'>Create the Toast webhook</span>
        <ChevronDown
          className={cn(
            "size-4 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-180"
          )}
        />
      </CollapsibleTrigger>

      <CollapsibleContent className='flex flex-col gap-3 px-3 pb-3'>
        <p className='text-muted-foreground text-xs leading-relaxed'>
          Toast pushes orders to Oxy via a webhook subscription. Create one in Toast Web, then paste
          the generated secret into the variable above (its value lives in the Secrets tab).
        </p>

        <ol className='flex list-decimal flex-col gap-1.5 pl-4 text-muted-foreground text-xs leading-relaxed'>
          <li>
            Open{" "}
            <a
              href={`${TOAST_WEBHOOKS_URL}/new`}
              target='_blank'
              rel='noreferrer'
              className='inline-flex items-center gap-0.5 text-primary hover:underline'
            >
              Integrations → Webhooks → Add subscription
              <ExternalLink className='size-3' />
            </a>{" "}
            in Toast Web.
          </li>
          <li>
            Pick your Standard API credentials (e.g. <span className='font-mono'>oxy-prod</span>)
            and subscribe to the <span className='font-mono'>order_updated</span> event (add{" "}
            <span className='font-mono'>channel_order_updated</span> if it appears).
          </li>
          <li>Set the subscription URL to the value below.</li>
          <li>
            After saving, Toast shows the <span className='font-medium'>Secret</span> once — copy it
            into the Secrets tab under the variable above (it can only be rotated, not viewed
            again).
          </li>
        </ol>

        <WebhookUrlField path={webhookPath} />
      </CollapsibleContent>
    </Collapsible>
  );
}

/**
 * The full webhook URL with a copy button. The base is the origin this UI is
 * served from (`DEFAULT_BASE_URL`); the URL wraps so the `/api/webhooks/...`
 * prefix stays visible instead of scrolling out of view behind the long
 * `project_id`.
 */
function WebhookUrlField({ path }: { path: string }) {
  const [copied, setCopied] = useState(false);

  const fullUrl = `${DEFAULT_BASE_URL.replace(/\/+$/, "")}${path}`;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(fullUrl);
      setCopied(true);
      toast.success("Copied webhook URL");
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };

  return (
    <div className='flex flex-col gap-1'>
      <p className='text-muted-foreground text-xs'>Webhook URL</p>
      <div className='relative rounded-md border bg-background py-2 pr-10 pl-3'>
        <code className='block whitespace-pre-wrap break-all font-mono text-xs leading-relaxed'>
          {fullUrl}
        </code>
        <Button
          variant='ghost'
          size='icon'
          onClick={handleCopy}
          aria-label='Copy webhook URL'
          className='absolute top-1 right-1 size-7'
        >
          {copied ? <Check className='size-3.5 text-primary' /> : <Copy className='size-3.5' />}
        </Button>
      </div>
    </div>
  );
}
