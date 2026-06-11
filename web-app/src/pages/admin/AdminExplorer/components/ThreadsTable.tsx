import { ExternalLink, MessageSquare } from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import ROUTES from "@/libs/utils/routes";
import type { ExplorerThread } from "@/services/api/adminExplorer";
import { ago, tenantLabel } from "../format";

/**
 * Cross-tenant thread results. Each row drills straight into the conversation
 * inside its workspace (operators can now open non-member workspaces). The
 * input snippet is the "what was this about?" at-a-glance read.
 */
export const ThreadsTable = ({ rows }: { rows: ExplorerThread[] }) => {
  if (rows.length === 0) {
    return <Empty />;
  }
  return (
    <div className='overflow-hidden rounded-lg border border-border/60 bg-card'>
      <div className='grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_auto_auto_auto] gap-3 border-border/60 border-b bg-muted/30 px-3 py-2 font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
        <span>Thread</span>
        <span>Workspace · Org</span>
        <span>User</span>
        <span className='w-16 text-right'>Age</span>
        <span className='w-10' aria-hidden />
      </div>
      <div className='divide-y divide-border/50'>
        {rows.map((t) => {
          const openable = t.org_slug && t.workspace_id;
          return (
            <div
              key={t.id}
              className='grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_auto_auto_auto] items-center gap-3 px-3 py-2 text-xs transition-colors hover:bg-muted/20'
            >
              <div className='min-w-0'>
                <div className='flex items-center gap-2'>
                  <MessageSquare className='size-3.5 shrink-0 text-muted-foreground' />
                  <span className='truncate font-medium'>{t.title || "(untitled)"}</span>
                  {t.is_processing ? (
                    <span className='shrink-0 rounded-full bg-primary/10 px-1.5 py-0.5 font-medium text-[9px] text-primary uppercase'>
                      live
                    </span>
                  ) : null}
                </div>
                {t.input_snippet ? (
                  <p className='truncate pl-5 text-[11px] text-muted-foreground'>
                    {t.input_snippet}
                  </p>
                ) : null}
              </div>
              <span className='truncate text-muted-foreground'>
                {tenantLabel(t.workspace_name, t.org_name)}
              </span>
              <span className='max-w-40 truncate font-mono text-[11px] text-muted-foreground'>
                {t.user_email ?? "—"}
              </span>
              <span className='w-16 text-right text-muted-foreground tabular-nums'>
                {ago(t.created_at)}
              </span>
              <div className='w-10 text-right'>
                {openable ? (
                  <Button
                    asChild
                    size='sm'
                    variant='ghost'
                    className='h-6 w-6 p-0 text-muted-foreground hover:text-foreground'
                  >
                    <Link
                      to={ROUTES.ORG(t.org_slug as string)
                        .WORKSPACE(t.workspace_id as string)
                        .THREAD(t.id)}
                      title='Open thread'
                    >
                      <ExternalLink className='size-3.5' />
                    </Link>
                  </Button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const Empty = () => (
  <div className='flex flex-col items-center justify-center gap-2 rounded-lg border border-border/60 border-dashed bg-muted/20 px-6 py-12 text-center'>
    <MessageSquare className='size-6 text-muted-foreground' />
    <p className='font-medium text-sm'>No threads match.</p>
    <p className='text-muted-foreground text-xs'>Search by title, content, or thread id.</p>
  </div>
);
