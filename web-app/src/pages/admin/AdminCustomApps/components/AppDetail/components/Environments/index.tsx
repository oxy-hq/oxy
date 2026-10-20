import { ExternalLink, FlaskConical, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  useAppEnvironments,
  useCreateAppEnvironment,
  useDeleteAppEnvironment
} from "@/hooks/api/customApps/useAppEnvironments";
import { isForbidden } from "@/hooks/api/httpStatus";
import { cn } from "@/libs/shadcn/utils";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { AppEnvironment } from "@/services/api/customApps";
import { LifecycleConfirm } from "../../../AppLifecycle";
import { shortBuildId } from "../../../AppLifecycle/lifecycleCopy";
import { relativeTime } from "../Activity/relativeTime";

/**
 * The app's environments — production, staging, and each developer's or agent's dev
 * sandbox — from `GET /customer-apps/{id}/environments`.
 *
 * This backend shipped with no UI at all: a sandbox could be created by `oxyc` and
 * torn down by its idle TTL, and nobody could see which existed or delete one early.
 * One list, one input, one verb. Production and staging are listed for the picture
 * but carry no Delete — the server refuses that with a 400, so the control is not
 * drawn rather than drawn and refused.
 */
export const Environments = ({ appId }: { appId: string }) => {
  const { data, isLoading, error } = useAppEnvironments(appId);
  const create = useCreateAppEnvironment(appId);
  const remove = useDeleteAppEnvironment(appId);
  const [name, setName] = useState("");
  const [deleting, setDeleting] = useState<AppEnvironment | null>(null);

  if (isLoading) return <Skeleton className='h-20 w-full' />;
  if (isForbidden(error)) {
    return (
      <p className='text-muted-foreground text-xs' data-testid='admin-app-environments-refused'>
        Environments are shown to Oxy staff who may open this app&rsquo;s non-production
        environments, and you may not.
      </p>
    );
  }
  if (error) {
    return (
      <p className='text-destructive text-xs' data-testid='admin-app-env-error'>
        Couldn't load environments.
      </p>
    );
  }

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    create.mutate(trimmed, { onSuccess: () => setName("") });
  };

  return (
    <div className='flex flex-col gap-3' data-testid='admin-app-environments'>
      <ul className='flex flex-col divide-y divide-border/50 rounded-md border'>
        {(data ?? []).map((env) => (
          <EnvironmentRow key={env.name} env={env} onDelete={() => setDeleting(env)} />
        ))}
      </ul>

      <form onSubmit={submit} className='flex items-center gap-2'>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder='dev-<handle>'
          className='h-7 max-w-xs font-mono text-xs'
          aria-label='Sandbox name'
          data-testid='admin-app-env-name'
        />
        <Button
          type='submit'
          size='sm'
          variant='outline'
          className='h-7 gap-1.5 text-xs'
          disabled={!name.trim() || create.isPending}
          data-testid='admin-app-env-create'
        >
          <FlaskConical className='size-3.5' />
          {create.isPending ? "Creating…" : "New sandbox"}
        </Button>
      </form>

      <LifecycleConfirm
        testId='admin-app-env-confirm'
        copy={
          deleting
            ? {
                title: `Delete sandbox ${deleting.name}?`,
                description:
                  "Its build, host and OLTP schema are torn down in the background. Anyone publishing to it gets a fresh sandbox next time.",
                confirm: "Delete"
              }
            : null
        }
        pending={remove.isPending}
        onConfirm={() =>
          deleting && remove.mutate(deleting.name, { onSuccess: () => setDeleting(null) })
        }
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
};

const EnvironmentRow = ({ env, onDelete }: { env: AppEnvironment; onDelete: () => void }) => {
  const sandbox = env.kind === "dev";
  const deletingNow = env.status === "deleting";
  return (
    <li
      className='flex items-center gap-3 px-3 py-2 text-xs'
      data-testid={`admin-app-env-row-${env.name}`}
    >
      <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <div className='flex items-center gap-2'>
          <span className='font-medium font-mono'>{env.name}</span>
          <span className='text-muted-foreground'>{env.kind}</span>
          {deletingNow && <span className={cn(ADMIN_TONE.warn.text)}>deleting</span>}
        </div>
        <span className='truncate text-muted-foreground'>
          {env.build_id ? `build ${shortBuildId(env.build_id)}` : "nothing published"}
          {env.owner?.email ? ` · ${env.owner.email}` : ""}
          {env.last_activity_at ? ` · active ${relativeTime(env.last_activity_at)}` : ""}
          {env.expires_at ? ` · expires ${relativeTime(env.expires_at).replace(" ago", "")}` : ""}
        </span>
      </div>
      {env.url && (
        <Button asChild variant='ghost' size='sm' className='h-6 gap-1 px-1.5 text-[11px]'>
          <a
            href={env.url}
            target='_blank'
            rel='noreferrer'
            data-testid={`admin-app-env-open-${env.name}`}
          >
            Open
            <ExternalLink className='size-3' />
          </a>
        </Button>
      )}
      {sandbox && (
        <Button
          variant='ghost'
          size='sm'
          className='h-6 gap-1 px-1.5 text-[11px] text-muted-foreground'
          disabled={deletingNow}
          onClick={onDelete}
          data-testid={`admin-app-env-delete-${env.name}`}
        >
          <Trash2 className='size-3' />
          Delete
        </Button>
      )}
    </li>
  );
};
