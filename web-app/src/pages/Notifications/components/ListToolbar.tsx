import { CheckCheck, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";

interface ListToolbarProps {
  unreadOnly: boolean;
  onUnreadOnly: (on: boolean) => void;
  canMarkAll: boolean;
  markingAll: boolean;
  onMarkAll: () => void;
  /** On a phone "Mark all read" is an icon. */
  compact: boolean;
}

/** Unread / All, and "Mark all read" for whatever the list is narrowed to. */
export function ListToolbar({
  unreadOnly,
  onUnreadOnly,
  canMarkAll,
  markingAll,
  onMarkAll,
  compact
}: ListToolbarProps) {
  return (
    <div className='flex shrink-0 items-center gap-2'>
      <ToggleGroup
        type='single'
        variant='outline'
        size='sm'
        value={unreadOnly ? "unread" : "all"}
        onValueChange={(v) => {
          if (v) onUnreadOnly(v === "unread");
        }}
        aria-label='Show'
      >
        <ToggleGroupItem value='unread' className='text-xs' data-testid='notifications-page-unread'>
          Unread
        </ToggleGroupItem>
        <ToggleGroupItem value='all' className='text-xs' data-testid='notifications-page-all'>
          All
        </ToggleGroupItem>
      </ToggleGroup>
      <Button
        size='sm'
        variant='ghost'
        disabled={!canMarkAll || markingAll}
        onClick={onMarkAll}
        aria-label={compact ? "Mark all read" : undefined}
        data-testid='notifications-page-mark-all-read'
        className='text-muted-foreground text-xs'
      >
        <CheckCheck className='size-4' />
        {!compact && "Mark all read"}
      </Button>
    </div>
  );
}

/** Says the list is one thread, and goes back to the whole list. */
export function ThreadChip({ onClear }: { onClear: () => void }) {
  return (
    <button
      type='button'
      onClick={onClear}
      data-testid='notifications-thread-clear'
      className='flex shrink-0 items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs'
    >
      One thread
      <X className='size-3' aria-hidden />
      <span className='sr-only'>, show all</span>
    </button>
  );
}
