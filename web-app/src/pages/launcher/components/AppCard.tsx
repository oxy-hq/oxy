import { useState } from "react";
import type { CustomAppSummary } from "@/types/apps";

export function AppCard({ app }: { app: CustomAppSummary }) {
  const [artFailed, setArtFailed] = useState(false);
  const [iconFailed, setIconFailed] = useState(false);
  const showArt = !!app.art_url && !artFailed;
  const showIcon = !!app.icon_url && !iconFailed;
  const initial = app.name.slice(0, 1).toUpperCase();
  return (
    <a
      href={app.url}
      data-testid={`launcher-app-card-${app.slug}`}
      className='group flex flex-col gap-3 overflow-hidden rounded-lg border bg-card p-5 transition-colors hover:border-primary/50'
    >
      {showArt ? (
        <img
          src={app.art_url}
          alt=''
          loading='lazy'
          onError={() => setArtFailed(true)}
          className='h-40 w-full rounded-md border object-cover'
        />
      ) : (
        <div className='flex h-40 items-center justify-center rounded-md bg-primary/10'>
          <span className='font-semibold text-4xl text-primary'>{initial}</span>
        </div>
      )}
      <div>
        {/* The mark sits beside the name — the same glyph the rail shows for this
            app — so the rail and the home cards read as one system. Kept off the
            art (which is the app's own screenshot) so it never clutters it. */}
        <div className='flex items-center gap-2'>
          {showIcon && (
            <img
              src={app.icon_url}
              alt=''
              onError={() => setIconFailed(true)}
              data-testid={`launcher-app-card-mark-${app.slug}`}
              className='h-6 w-6 shrink-0 rounded-md border object-contain'
            />
          )}
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
