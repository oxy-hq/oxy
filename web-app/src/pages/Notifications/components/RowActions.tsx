import { FollowLink } from "@/components/Notifications/FollowLink";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import { useMarkNotificationRead, useRunNotificationAction } from "@/hooks/api/notifications";
import { cn } from "@/libs/shadcn/utils";
import type { InboxItem, NotificationAction } from "@/types/notifications";

const BUTTON = "h-7 px-2.5 text-xs";

/** An action that runs the app's function, as the signed-in person. Its own pending state. */
function RunButton({ item, action }: { item: InboxItem; action: NotificationAction }) {
  const run = useRunNotificationAction();
  return (
    <Button
      size='sm'
      variant='outline'
      disabled={run.isPending}
      aria-busy={run.isPending}
      onClick={() => run.mutate({ item, actionId: action.id })}
      data-testid={`notifications-action-${item.id}-${action.id}`}
      className={cn(BUTTON, action.destructive && "text-destructive")}
    >
      {run.isPending && <Spinner className='size-3' />}
      {action.label}
    </Button>
  );
}

/**
 * An "open" action follows the notification's own link, the way the row does,
 * and marks it read — a link rather than a call, so an app's tab opens on the
 * click itself and no popup blocker stands in the way.
 */
function OpenLink({ item, action }: { item: InboxItem; action: NotificationAction }) {
  const markRead = useMarkNotificationRead();
  if (!item.href) return null;
  return (
    <FollowLink
      href={item.href}
      onClick={() => {
        if (item.read_at === null) markRead.mutate(item);
      }}
      data-testid={`notifications-action-${item.id}-${action.id}`}
      className={cn(buttonVariants({ variant: "outline", size: "sm" }), BUTTON)}
    >
      {action.label}
    </FollowLink>
  );
}

/** A notification's buttons, from its kind. Nothing from a server that sends none. */
export function RowActions({ item }: { item: InboxItem }) {
  const actions = item.actions ?? [];
  if (actions.length === 0) return null;
  return (
    <>
      {actions.map((action) =>
        action.runs_function ? (
          <RunButton key={action.id} item={item} action={action} />
        ) : (
          <OpenLink key={action.id} item={item} action={action} />
        )
      )}
    </>
  );
}
