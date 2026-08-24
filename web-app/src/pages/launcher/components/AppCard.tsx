import { ExternalLink, ShieldCheck } from "lucide-react";
// Direct, not via the barrel: a card that shows a badge shouldn't declare a
// dependency on the dialog and its pickers. (No bundle saving today — the app is a
// single chunk and the launcher imports the dialog anyway — so this is coupling,
// not bytes.)
import { AppAccessBadge } from "@/components/appAccess/AppAccessBadge";
import { AppArt } from "@/components/apps/AppArt";
import { AppMark } from "@/components/apps/AppMark";
import { Button } from "@/components/ui/shadcn/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { prefetchApp } from "@/libs/utils/prefetchApp";
import useAppDock from "@/stores/useAppDock";
import type { CustomAppSummary } from "@/types/apps";

/**
 * True when a click carries a modifier that means "not here" — a new tab, a new
 * window, a background tab, or a middle click.
 *
 * These have to be honoured by letting the anchor's default behaviour run.
 * Intercepting them is the single most common way an in-app router breaks
 * cmd-click, and people notice immediately.
 */
function wantsNewContext(e: React.MouseEvent): boolean {
  return e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0;
}

export function AppCard({
  app,
  onManageAccess
}: {
  app: CustomAppSummary;
  /**
   * Open the access dialog for this app. Passed only for org owners and admins —
   * absent means no control renders at all, which is the right default for the
   * home page: everyone else's cards stay exactly as they were.
   */
  onManageAccess?: (app: CustomAppSummary) => void;
}) {
  const openInDock = useAppDock((s) => s.open);

  // Hover and keyboard focus both mean "about to open this": a card reached by
  // Tab is one Enter away, and warming on focus is what keeps the keyboard path
  // as fast as the mouse one. `prefetchApp` uses a `rel=prefetch` link rather
  // than a `fetch()` precisely so the serve path can tell a hover from an open
  // and not record a view for it.
  const warm = () => prefetchApp(app.url);

  return (
    // The card is a div, not an anchor, because it holds two more actions. The
    // name's link stretches over the whole card via `after:inset-0`, so the card
    // is still one big click target — but the buttons are siblings of that link
    // rather than nested inside it, which is what keeps them from navigating
    // (and keeps the markup valid).
    <div className='group relative flex flex-col gap-3 overflow-hidden rounded-lg border bg-card p-5 transition-colors focus-within:border-primary/50 hover:border-primary/50'>
      <AppArt artUrl={app.art_url} name={app.name} />
      <div>
        {/* The mark sits beside the name — the same glyph the rail shows for this
            app — so the rail and the home cards read as one system. Kept off the
            art (which is the app's own screenshot) so it never clutters it. */}
        <div className='flex items-center gap-2'>
          <AppMark
            iconUrl={app.icon_url}
            name={app.name}
            size='md'
            testId={`launcher-app-card-mark-${app.slug}`}
          />
          <h3 className='font-semibold text-base text-card-foreground'>
            {/* Still a real anchor with a real href, even though a plain click is
                handled in-page. That is what makes cmd-click, middle-click,
                "open in new tab" and "copy link address" work — and it is why
                the modifier check defers to the default rather than
                re-implementing any of them. */}
            <a
              href={app.url}
              data-testid={`launcher-app-card-${app.slug}`}
              className='after:absolute after:inset-0 after:content-[""]'
              // On the link, not the card: the stretched `::after` makes the
              // anchor's hit area the whole card, so this still fires wherever
              // the pointer enters — and the element being warmed is the one
              // that is about to be activated.
              onPointerEnter={warm}
              onFocus={warm}
              onClick={(e) => {
                if (wantsNewContext(e)) return;
                e.preventDefault();
                openInDock(app);
              }}
            >
              {app.name}
            </a>
          </h3>
        </div>
        {app.description && (
          <p className='mt-1 line-clamp-2 text-muted-foreground text-sm'>{app.description}</p>
        )}
      </div>
      <div className='mt-auto flex items-center gap-2 border-border/60 border-t pt-3'>
        {app.status && (
          <span className='min-w-0 flex-1 truncate text-muted-foreground/80 text-xs'>
            {app.status}
          </span>
        )}
        {/* `z-10` lifts these above the stretched link's overlay — without it
            the link swallows the click and the buttons silently navigate. */}
        <div className='relative z-10 ml-auto flex shrink-0 items-center gap-1.5'>
          {onManageAccess && (
            <>
              {/* Only the restricted state gets a badge. "Whole org" is the
                  default and true of most cards, so labelling it would put a
                  chip on every tile to say nothing. */}
              {app.visibility === "members" && <AppAccessBadge visibility={app.visibility} />}
              <Button
                variant='ghost'
                size='sm'
                className='h-6 gap-1 px-1.5 text-xs'
                onClick={() => onManageAccess(app)}
              >
                <ShieldCheck className='size-3' aria-hidden />
                Access
              </Button>
            </>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <a
                href={app.url}
                target='_blank'
                rel='noreferrer'
                aria-label={`Open ${app.name} in a new tab`}
                data-testid={`launcher-app-card-external-${app.slug}`}
                className='inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground'
              >
                <ExternalLink className='size-3.5' aria-hidden />
              </a>
            </TooltipTrigger>
            <TooltipContent side='bottom'>Open in a new tab</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </div>
  );
}
