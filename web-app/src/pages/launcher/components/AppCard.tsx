import { AppArt } from "@/components/apps/AppArt";
import { AppMark } from "@/components/apps/AppMark";
import type { CustomAppSummary } from "@/types/apps";

export function AppCard({ app }: { app: CustomAppSummary }) {
  return (
    <a
      href={app.url}
      data-testid={`launcher-app-card-${app.slug}`}
      className='group flex flex-col gap-3 overflow-hidden rounded-lg border bg-card p-5 transition-colors hover:border-primary/50'
    >
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
          <h3 className='font-semibold text-base text-card-foreground'>{app.name}</h3>
        </div>
        {app.description && (
          <p className='mt-1 line-clamp-2 text-muted-foreground text-sm'>{app.description}</p>
        )}
      </div>
      {app.status && (
        <div className='mt-auto border-border/60 border-t pt-3 text-muted-foreground/80 text-xs'>
          {app.status}
        </div>
      )}
    </a>
  );
}
