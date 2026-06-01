import { AlertCircle, CheckCircle2 } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppDebug } from "@/hooks/api/customerApps/useCustomerApps";
import type { CustomerApp } from "@/types/apps";

/**
 * Inspect-only view of what oxy currently resolves for the selected
 * app: bundle dir, manifest source, raw manifest blob. Calls `/debug`
 * (the diagnostic endpoint we ship for the SDK too). Read-only by
 * design — Settings tab owns the mutation surface.
 *
 * Project + branch come from the admin row (`app` prop) rather than
 * the debug snapshot: in v2 the debug surface is bundle-public and
 * deliberately excludes admin-grade fields.
 */
export const AppInfo = ({ app }: { app: CustomerApp }) => {
  const { data, isLoading, error } = useAppDebug(app.org_slug, app.slug);

  if (isLoading) {
    return (
      <div className='space-y-4 p-6'>
        <Skeleton className='h-32 w-full' />
        <Skeleton className='h-48 w-full' />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className='flex items-center gap-2 p-6 text-destructive text-sm'>
        <AlertCircle className='size-4' />
        <span>Failed to load diagnostic snapshot.</span>
      </div>
    );
  }

  const manifestOk = !!data.manifest && !data.manifest_error;
  const dirOk = data.bundle_dir_exists;

  return (
    <div className='space-y-6 p-6'>
      {/* Status row — three pills the operator scans first */}
      <div className='grid grid-cols-3 gap-3'>
        <StatusPill
          label='Bundle dir'
          ok={dirOk}
          value={dirOk ? "exists" : "missing"}
          subtle={data.bundle_dir ?? "—"}
        />
        <StatusPill
          label='Manifest'
          ok={manifestOk}
          value={data.manifest_source === "db_override" ? "DB override" : "Bundled file"}
          subtle={manifestOk ? "loaded" : (data.manifest_error ?? "unknown")}
        />
        <StatusPill
          label='Source'
          ok={true}
          value={data.app.source_type}
          subtle={`${app.branch} branch`}
        />
      </div>

      {/* Identity card */}
      <Section title='Identity'>
        <KV k='App ID' v={data.app.id} mono />
        <KV k='Project' v={app.project_id} mono />
        <KV k='Status' v={data.app.status} />
      </Section>

      {/* Manifest error if any */}
      {data.manifest_error && (
        <Section title='Manifest error' tone='destructive'>
          <pre className='whitespace-pre-wrap rounded-md bg-destructive/10 p-3 text-destructive text-xs'>
            {data.manifest_error}
          </pre>
        </Section>
      )}

      {/* Raw manifest blob — loose by design (v2 server returns it as
          opaque JSON; the bundle owns its structure). */}
      {manifestOk && (
        <Section title='Manifest'>
          <pre className='max-h-64 overflow-auto whitespace-pre-wrap rounded-md border bg-muted/40 p-3 font-mono text-xs'>
            {JSON.stringify(data.manifest, null, 2)}
          </pre>
        </Section>
      )}
    </div>
  );
};

const StatusPill = ({
  label,
  ok,
  value,
  subtle
}: {
  label: string;
  ok: boolean;
  value: string;
  subtle: string;
}) => (
  <div className='rounded-md border bg-card p-3'>
    <div className='flex items-center justify-between'>
      <span className='font-medium text-muted-foreground text-xs uppercase tracking-wider'>
        {label}
      </span>
      {ok ? (
        <CheckCircle2 className='size-3.5 text-emerald-500' />
      ) : (
        <AlertCircle className='size-3.5 text-destructive' />
      )}
    </div>
    <div className='mt-1.5 truncate font-medium text-sm'>{value}</div>
    <div className='mt-0.5 truncate font-mono text-muted-foreground text-xs'>{subtle}</div>
  </div>
);

const Section = ({
  title,
  children,
  tone
}: {
  title: string;
  children: React.ReactNode;
  tone?: "destructive";
}) => (
  <div>
    <h3
      className={`mb-2 font-medium text-xs uppercase tracking-wider ${
        tone === "destructive" ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      {title}
    </h3>
    <div className='space-y-2'>{children}</div>
  </div>
);

const KV = ({ k, v, mono }: { k: string; v: string; mono?: boolean }) => (
  <div className='flex items-baseline justify-between gap-4 border-b py-1.5 text-sm last:border-0'>
    <span className='text-muted-foreground'>{k}</span>
    <span className={mono ? "font-mono text-xs" : ""}>{v}</span>
  </div>
);
