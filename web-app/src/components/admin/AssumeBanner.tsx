import { ShieldAlert } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { useEndAssume } from "@/hooks/api/adminAssume";
import { useActingSession } from "@/hooks/api/adminAssume/useActingSession";

function remaining(seconds: number): string {
  if (seconds <= 0) return "expiring";
  const m = Math.floor(seconds / 60);
  return m >= 1 ? `${m} min left` : `${seconds}s left`;
}

/**
 * The "you are impersonating someone" banner.
 *
 * Acting as a tenant is a **mode**, not a badge: the server refuses the staff
 * surface while it's live, and you were dropped into the tenant's own product. So
 * this banner is the only thing on screen that still belongs to *you* — it has to
 * be persistent, undismissable, and carry the one-click exit, because the fastest
 * way out of someone else's account should always be in front of you.
 *
 * Rendered app-wide (not just under /admin): the whole point is that it follows
 * you INTO the tenant, which is where the damage would be done.
 */
export function AssumeBanner() {
  const { session, home } = useActingSession();
  const end = useEndAssume();

  if (!session) return null;

  const what = session.is_partner
    ? `${session.org_name ?? "a partner"} — as its partner admin`
    : `${session.org_name ?? "an organization"}`;
  // Whichever console you came from is closed for the duration — naming the right
  // one matters, because a partner has never seen /admin and telling them it is
  // closed would be gibberish.
  const closed = home === "/admin/tenants" ? "admin is closed" : "your partner console is closed";
  const back = home === "/admin/tenants" ? "Stop & return to admin" : "Stop & return to console";

  // Fixed, not in flow: `.root` is a fixed flex container, so an in-flow banner
  // would collapse the app's width. Its height is reserved via `body.is-acting`
  // in index.css — h-8 must stay in sync with --acting-banner-h.
  return (
    <div
      data-testid='assume-banner'
      className='fixed inset-x-0 top-0 z-50 flex h-8 items-center gap-2 border-amber-500/40 border-b bg-amber-500/15 px-3 text-amber-900 dark:text-amber-200'
    >
      <ShieldAlert className='size-4 shrink-0' />
      <p className='min-w-0 flex-1 truncate text-xs'>
        <span className='font-semibold'>You are acting as {what}</span>
        <span className='opacity-80'>
          {" — "}
          you are {session.actor_email} · {remaining(session.expires_in_seconds)} · “
          {session.reason}” · {closed} until you stop · this session is audited
        </span>
      </p>
      <Button
        size='sm'
        variant='outline'
        className='h-6 shrink-0 border-amber-600/40 bg-background/60 text-xs'
        disabled={end.isPending}
        onClick={() => end.mutate(session.org_id)}
      >
        {back}
      </Button>
    </div>
  );
}
