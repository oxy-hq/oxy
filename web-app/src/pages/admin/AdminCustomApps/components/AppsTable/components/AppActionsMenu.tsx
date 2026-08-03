import { Copy, ExternalLink, EyeOff, MoreHorizontal, PanelRightOpen, Rocket } from "lucide-react";
import { toast } from "sonner";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { cn } from "@/libs/shadcn/utils";
import type { CustomApp } from "@/types/apps";
import { resolveBundleUrl } from "../../../resolveBundleUrl";

interface AppActionsMenuProps {
  app: CustomApp;
  onOpen: (app: CustomApp) => void;
  onPublish: (app: CustomApp) => void;
  onUnpublish: (app: CustomApp) => void;
  /** Extra classes for the trigger (size/positioning differs card vs row). */
  triggerClassName?: string;
}

const copy = async (label: string, value: string) => {
  try {
    await navigator.clipboard.writeText(value);
    toast.success(`Copied ${label}`);
  } catch {
    toast.error("Couldn't copy to clipboard");
  }
};

/** The ⋯ menu shared by the list row and the gallery card. */
export const AppActionsMenu = ({
  app,
  onOpen,
  onPublish,
  onUnpublish,
  triggerClassName
}: AppActionsMenuProps) => {
  const isLive = !!app.published_at;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={cn(
          "inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          triggerClassName
        )}
        aria-label={`Actions for ${app.name}`}
        onClick={(e) => e.stopPropagation()}
      >
        <MoreHorizontal className='size-3.5' />
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' className='w-52' onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem onClick={() => onOpen(app)}>
          <PanelRightOpen className='size-3.5' />
          Open details
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => window.open(resolveBundleUrl(app.url), "_blank", "noopener")}
        >
          <ExternalLink className='size-3.5' />
          Open app in new tab
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => copy("app URL", resolveBundleUrl(app.url))}>
          <Copy className='size-3.5' />
          Copy URL
        </DropdownMenuItem>
        {app.url_subdomain && (
          <DropdownMenuItem onClick={() => copy("subdomain URL", app.url_subdomain as string)}>
            <Copy className='size-3.5' />
            Copy subdomain URL
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        {isLive ? (
          <DropdownMenuItem onClick={() => onUnpublish(app)}>
            <EyeOff className='size-3.5' />
            Unpublish
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem onClick={() => onPublish(app)}>
            <Rocket className='size-3.5' />
            Publish
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

/** Filled brand dot = Live; hollow ring = Draft. Emerald stays reserved for
 *  workflow-node success, so Live leans on the brand `primary` token. */
export const StatusPill = ({ isLive }: { isLive: boolean }) => (
  <span className='inline-flex items-center gap-1.5 text-xs'>
    <span
      className={cn(
        "size-2 rounded-full",
        isLive ? "bg-primary" : "border border-muted-foreground/50"
      )}
      aria-hidden
    />
    <span className={cn(isLive ? "text-foreground" : "text-muted-foreground")}>
      {isLive ? "Live" : "Draft"}
    </span>
  </span>
);
