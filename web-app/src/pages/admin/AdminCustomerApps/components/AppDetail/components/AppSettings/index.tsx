import {
  CheckCircle2,
  ExternalLink,
  EyeOff,
  FolderOpen,
  Send,
  Trash2,
  Triangle
} from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { usePublishApp, useUnpublishApp } from "@/hooks/api/customApps/useCustomApps";
import { useUpdateApp } from "@/hooks/api/customerApps/useCustomerApps";
import { useDeleteApp } from "@/hooks/api/customerApps/useDeleteApp";
import type { CustomerApp } from "@/types/apps";

function formatTimestamp(value: string): string {
  return new Date(value).toLocaleString();
}

/**
 * Read-then-act surface for one app. Three rows: re-sync (S3 only),
 * delete, and a pointer to the bootstrap PR if one exists.
 *
 * v1 is intentionally minimal — manifest_override editing, branch
 * changes, role grants etc. land in follow-ups. Today this just
 * surfaces what the existing API already supports.
 */
export const AppSettings = ({ app }: { app: CustomerApp }) => {
  const navigate = useNavigate();
  const { mutate: del, isPending: isDeleting } = useDeleteApp();
  const { mutate: publish, isPending: isPublishing } = usePublishApp();
  const { mutate: unpublish, isPending: isUnpublishing } = useUnpublishApp();
  const isPublished = !!app.published_at;

  const handleDelete = () => {
    if (
      !window.confirm(
        `Delete "${app.name}"? The bundle in oxy-hq/customer-apps stays — only the registration row goes.`
      )
    ) {
      return;
    }
    del(app.id, {
      onSuccess: () => navigate("/admin/apps")
    });
  };

  return (
    <div className='space-y-4 p-4'>
      <SettingRow
        title={isPublished ? "Published" : "Draft"}
        description={
          isPublished
            ? `Live for members of org ${app.org_slug}.`
            : "Only Oxy staff can reach this app. Publish to show it in the customer sidebar."
        }
        meta={app.published_at ? `Last published ${formatTimestamp(app.published_at)}` : undefined}
        tone={isPublished ? "success" : undefined}
        action={
          isPublished ? (
            <>
              <Button
                variant='outline'
                size='sm'
                disabled={isPublishing}
                onClick={() => publish(app.id)}
              >
                <Send className='size-3.5' />
                {isPublishing ? "Re-publishing…" : "Re-publish"}
              </Button>
              <Button
                variant='ghost'
                size='sm'
                disabled={isUnpublishing}
                onClick={() => unpublish(app.id)}
              >
                <EyeOff className='size-3.5' />
                {isUnpublishing ? "Unpublishing…" : "Unpublish"}
              </Button>
            </>
          ) : (
            <Button size='sm' disabled={isPublishing} onClick={() => publish(app.id)}>
              <CheckCircle2 className='size-3.5' />
              {isPublishing ? "Publishing…" : "Publish"}
            </Button>
          )
        }
      />

      {/* LocalFolder path editor. Only meaningful for `local` source —
          the file the admin needs to point oxy at lives on the box
          running the server. Most useful for fixing a wrong-folder
          mistake without delete + recreate. */}
      {app.source_type === "local" && <LocalBundlePathRow app={app} />}

      {app.bootstrap_pr_url && (
        <SettingRow
          title='Bootstrap PR'
          description='Merge to seed the customer-apps repo.'
          action={
            <Button variant='outline' size='sm' asChild>
              <a href={app.bootstrap_pr_url} target='_blank' rel='noopener noreferrer'>
                <Triangle className='size-3.5' />
                Open PR
                <ExternalLink className='size-3.5' />
              </a>
            </Button>
          }
        />
      )}

      <SettingRow
        title='Delete registration'
        description='Removes the app row. The bundle source stays — clean that up separately.'
        tone='destructive'
        action={
          <Button variant='destructive' size='sm' disabled={isDeleting} onClick={handleDelete}>
            <Trash2 className='size-3.5' />
            {isDeleting ? "Deleting…" : "Delete"}
          </Button>
        }
      />
    </div>
  );
};

const SettingRow = ({
  title,
  description,
  action,
  tone,
  disabledNote,
  meta
}: {
  title: string;
  description: string;
  action: React.ReactNode;
  tone?: "destructive" | "success";
  disabledNote?: string;
  meta?: string;
}) => {
  const toneClass =
    tone === "destructive"
      ? "border-destructive/30 bg-destructive/5"
      : tone === "success"
        ? "border-emerald-500/30 bg-emerald-500/5"
        : "bg-card";
  const titleToneClass =
    tone === "destructive" ? "text-destructive" : tone === "success" ? "text-emerald-600" : "";
  return (
    // Stacked, not side-by-side: the dossier is a narrow resizable column (and
    // an overlay Sheet on narrow viewports), so a title|action row can't hold a
    // two-button action without overflowing. The action sits below the copy and
    // wraps.
    <div className={`rounded-lg border p-4 ${toneClass}`}>
      <div className='min-w-0'>
        <div className={`font-medium text-sm ${titleToneClass}`}>{title}</div>
        <p className='mt-1 text-muted-foreground text-sm leading-relaxed'>{description}</p>
        {meta && (
          <p className='mt-2 font-mono text-muted-foreground text-xs tabular-nums'>{meta}</p>
        )}
        {disabledNote && (
          <p className='mt-2 font-mono text-muted-foreground text-xs'>{disabledNote}</p>
        )}
      </div>
      <div className='mt-3 flex flex-wrap items-center gap-2'>{action}</div>
    </div>
  );
};

/**
 * Inline editor for `LocalFolder` source's `path`. Shows the current
 * path as a mono caption; click Edit to swap in a text input + Save /
 * Cancel. The path itself is a server-side filesystem path (oxy reads
 * `<path>/index.html` and assets straight from disk), so the field is
 * intentionally a free-form text input — there's no browser-side
 * directory picker we can offer that maps to the server's view.
 */
const LocalBundlePathRow = ({ app }: { app: CustomerApp }) => {
  const currentPath =
    typeof app.source_config === "object" && app.source_config !== null
      ? String((app.source_config as Record<string, unknown>).path ?? "")
      : "";

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(currentPath);
  const { mutate: update, isPending } = useUpdateApp();

  const save = () => {
    const trimmed = draft.trim();
    if (!trimmed || trimmed === currentPath) {
      setEditing(false);
      return;
    }
    update(
      { id: app.id, req: { source: { type: "local", path: trimmed } } },
      { onSuccess: () => setEditing(false) }
    );
  };

  const cancel = () => {
    setDraft(currentPath);
    setEditing(false);
  };

  return (
    <div className='rounded-lg border bg-card p-4'>
      <div className='flex items-start justify-between gap-6'>
        <div className='min-w-0 flex-1'>
          <div className='flex items-center gap-2 font-medium text-sm'>
            <FolderOpen className='size-3.5 text-muted-foreground' />
            Local bundle path
          </div>
          <p className='mt-1 text-muted-foreground text-sm leading-relaxed'>
            Absolute path to the built bundle folder on this host.
          </p>
          {!editing && (
            <p className='mt-2 break-all font-mono text-muted-foreground text-xs tabular-nums'>
              {currentPath || "— not set —"}
            </p>
          )}
          {editing && (
            <div className='mt-3 flex flex-col gap-2 sm:flex-row sm:items-center'>
              <Input
                autoFocus
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                placeholder='/Users/you/path/to/app/out'
                disabled={isPending}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    save();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    cancel();
                  }
                }}
                className='font-mono text-xs'
              />
              <div className='flex shrink-0 gap-2'>
                <Button size='sm' disabled={isPending} onClick={save}>
                  {isPending ? "Saving…" : "Save"}
                </Button>
                <Button variant='ghost' size='sm' disabled={isPending} onClick={cancel}>
                  Cancel
                </Button>
              </div>
            </div>
          )}
        </div>
        {!editing && (
          <div className='shrink-0'>
            <Button variant='outline' size='sm' onClick={() => setEditing(true)}>
              {currentPath ? "Edit" : "Set path"}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
};
