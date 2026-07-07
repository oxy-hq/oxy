import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../resolveBundleUrl";
import { AppDetail } from "./AppDetail";
import { AppFavicon } from "./AppFavicon";
import { CopyButton, OpenAppButton } from "./AppsTable/components/UrlActions";
import { formatRelativeTime } from "./AppsTable/useAppsTable";

/**
 * Full-page app detail — the deployment-platform pattern (Vercel's project
 * page) rather than a slide-over. A slim back-bar carries the identity + the
 * copy-URL / open quick actions; the rich dossier (preview + builds + activity
 * + settings) gets the full width below it.
 */
export const AppDetailPage = ({ app, onBack }: { app: CustomerApp; onBack: () => void }) => (
  <div className='flex h-full min-h-0 flex-col'>
    <header className='flex shrink-0 items-center gap-3 border-b px-4 py-2.5'>
      <Button variant='ghost' size='sm' className='h-8 gap-1.5' onClick={onBack}>
        <ArrowLeft className='size-4' />
        Apps
      </Button>
      <AppFavicon app={app} size='lg' />
      <div className='min-w-0'>
        <h1 className='truncate font-semibold text-base leading-tight'>{app.name}</h1>
        <p className='truncate font-mono text-muted-foreground text-xs'>
          {app.org_slug}/{app.slug}
          {app.last_promoted_by_email && (
            <span className='text-muted-foreground/70'>
              {" · promoted by "}
              {app.last_promoted_by_email}
              {app.last_promoted_at ? ` · ${formatRelativeTime(app.last_promoted_at)}` : ""}
            </span>
          )}
        </p>
      </div>
      <div className='ml-auto flex shrink-0 items-center gap-1'>
        <CopyButton value={resolveBundleUrl(app.url)} label='app URL' />
        <OpenAppButton url={app.url} />
      </div>
    </header>
    <div className='min-h-0 flex-1'>
      <AppDetail app={app} />
    </div>
  </div>
);
