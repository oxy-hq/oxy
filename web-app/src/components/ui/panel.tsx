import { X } from "lucide-react";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/libs/shadcn/utils";
import { Button } from "./shadcn/button";

// ---------------------------------------------------------------------------
// Panel — right-side detail panel shell
// ---------------------------------------------------------------------------

interface PanelProps extends HTMLAttributes<HTMLDivElement> {
  /** Adds slide-in-from-right animation (for overlay/fixed panels) */
  animate?: boolean;
}

function Panel({ className, animate, ...props }: PanelProps) {
  return (
    <div
      data-slot='panel'
      className={cn(
        "flex h-full flex-col bg-background",
        animate && "slide-in-from-right animate-in duration-200",
        className
      )}
      {...props}
    />
  );
}

// ---------------------------------------------------------------------------
// PanelHeader — consistent header bar with title, subtitle, actions, close
// ---------------------------------------------------------------------------

interface PanelHeaderProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** String renders as <h3 className="truncate font-semibold text-sm">; ReactNode renders as-is */
  title: ReactNode;
  /** String renders as <p className="text-muted-foreground text-xs">; ReactNode renders as-is */
  subtitle?: ReactNode;
  /** Extra action buttons rendered before the close button */
  actions?: ReactNode;
  /** When provided, renders an X close button */
  onClose?: () => void;
}

function PanelHeader({ title, subtitle, onClose, actions, className, ...props }: PanelHeaderProps) {
  return (
    <div
      data-slot='panel-header'
      className={cn("flex shrink-0 items-center justify-between border-b px-4 py-3", className)}
      {...props}
    >
      <div className='min-w-0 flex-1'>
        {typeof title === "string" ? (
          <h3 className='truncate font-semibold text-sm'>{title}</h3>
        ) : (
          title
        )}
        {subtitle &&
          (typeof subtitle === "string" ? (
            <p className='text-muted-foreground text-xs'>{subtitle}</p>
          ) : (
            subtitle
          ))}
      </div>
      <div className='flex shrink-0 items-center gap-1'>
        {actions}
        {onClose && (
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7 shrink-0'
            onClick={onClose}
            aria-label='Close panel'
          >
            <X className='h-4 w-4' />
          </Button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// PanelContent — flex-1 scrollable content area
// ---------------------------------------------------------------------------

interface PanelContentProps extends HTMLAttributes<HTMLDivElement> {
  /** Whether to add overflow-auto (default: true) */
  scrollable?: boolean;
  /** Whether to add p-4 padding (default: true) */
  padding?: boolean;
}

function PanelContent({
  className,
  scrollable = true,
  padding = true,
  ...props
}: PanelContentProps) {
  return (
    <div
      data-slot='panel-content'
      className={cn(
        "flex-1",
        scrollable ? "overflow-auto" : "overflow-hidden",
        padding && "p-4",
        className
      )}
      {...props}
    />
  );
}

export { Panel, PanelContent, PanelHeader };
