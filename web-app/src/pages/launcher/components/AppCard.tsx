import { ShieldCheck } from "lucide-react";
// Direct, not via the barrel: a card that shows a badge shouldn't declare a
// dependency on the dialog and its pickers. (No bundle saving today — the app is a
// single chunk and the launcher imports the dialog anyway — so this is coupling,
// not bytes.)
import { AppAccessBadge } from "@/components/appAccess/AppAccessBadge";
import { AppArt } from "@/components/apps/AppArt";
import { AppMark } from "@/components/apps/AppMark";
import { Button } from "@/components/ui/shadcn/button";
import type { CustomAppSummary } from "@/types/apps";

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
  return (
    // The card is a div, not an anchor, because it now holds a second action.
    // The name's link stretches over the whole card via `after:inset-0`, so the
    // card is still one big click target — but the access button is a sibling of
    // that link rather than nested inside it, which is what keeps it from
    // navigating (and keeps the markup valid).
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
            <a
              href={app.url}
              data-testid={`launcher-app-card-${app.slug}`}
              className='after:absolute after:inset-0 after:content-[""]'
            >
              {app.name}
            </a>
          </h3>
        </div>
        {app.description && (
          <p className='mt-1 line-clamp-2 text-muted-foreground text-sm'>{app.description}</p>
        )}
      </div>
      {(app.status || onManageAccess) && (
        <div className='mt-auto flex items-center gap-2 border-border/60 border-t pt-3'>
          {app.status && (
            <span className='min-w-0 flex-1 truncate text-muted-foreground/80 text-xs'>
              {app.status}
            </span>
          )}
          {onManageAccess && (
            // `z-10` lifts these above the stretched link's overlay — without it
            // the link swallows the click and the button silently navigates.
            <div className='relative z-10 ml-auto flex shrink-0 items-center gap-1.5'>
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
            </div>
          )}
        </div>
      )}
    </div>
  );
}
