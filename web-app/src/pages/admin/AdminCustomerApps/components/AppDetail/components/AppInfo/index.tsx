import { AlertCircle, CheckCircle2, Copy, ExternalLink } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppDebug } from "@/hooks/api/customerApps/useCustomerApps";
import { resolveBundleUrl } from "@/pages/admin/AdminCustomerApps/resolveBundleUrl";
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

  const isRemote = data.manifest_source === "remote";
  const manifestOk = isRemote || (!!data.manifest && !data.manifest_error);
  // Remote bundles have no oxy-side filesystem — bundle-dir checks don't
  // apply, so treat them as OK and show the upstream URL instead.
  const dirOk = isRemote || data.bundle_dir_exists;

  return (
    <div className='space-y-6 p-6'>
      {/* Status row — three pills the operator scans first */}
      <div className='grid grid-cols-3 gap-3'>
        <StatusPill
          label='Bundle dir'
          ok={dirOk}
          value={isRemote ? "remote" : dirOk ? "exists" : "missing"}
          subtle={isRemote ? (data.upstream_url ?? "—") : (data.bundle_dir ?? "—")}
        />
        <StatusPill
          label='Manifest'
          ok={manifestOk}
          value={
            isRemote
              ? "external"
              : data.manifest_source === "db_override"
                ? "DB override"
                : "Bundled file"
          }
          subtle={
            isRemote
              ? "owned by upstream"
              : manifestOk
                ? "loaded"
                : (data.manifest_error ?? "unknown")
          }
        />
        <StatusPill
          label='Source'
          ok={true}
          value={data.app.source_type}
          subtle={`${app.branch} branch`}
        />
      </div>

      {/* URLs — what to share with the customer or use in iframes.
          Subdomain URL is preferred for v0 sources when present
          (root-relative URLs in the upstream resolve correctly with
          zero per-app config); subpath URL always works and is the
          only option for S3-source apps. */}
      <Section title='URLs'>
        <UrlRow label='Subpath URL' url={app.url} />
        {app.url_subdomain && (
          <UrlRow label='Subdomain URL' url={app.url_subdomain} recommended absolute />
        )}
      </Section>

      {/* Identity card */}
      <Section title='Identity'>
        <KV k='App ID' v={data.app.id} mono />
        <KV k='Project' v={app.project_id} mono />
        <KV k='Status' v={data.app.status} />
      </Section>

      {/* Manifest error if any — never shown for remote bundles since
          there's no oxy-side manifest to fail. */}
      {!isRemote && data.manifest_error && (
        <Section title='Manifest error' tone='destructive'>
          <pre className='whitespace-pre-wrap rounded-md bg-destructive/10 p-3 text-destructive text-xs'>
            {data.manifest_error}
          </pre>
        </Section>
      )}

      {/* Raw manifest blob — loose by design (v2 server returns it as
          opaque JSON; the bundle owns its structure). Skipped for remote
          bundles. */}
      {!isRemote && manifestOk && (
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

const UrlRow = ({
  label,
  url,
  recommended,
  absolute
}: {
  label: string;
  url: string;
  /** Show a small "Recommended" pill — used for the v0 subdomain URL. */
  recommended?: boolean;
  /**
   * When true, open the URL as-is (it's already absolute, e.g. the v0
   * subdomain). Otherwise resolve against the current origin so a
   * relative URL like `/customer-apps/o/s/` opens on the admin host.
   */
  absolute?: boolean;
}) => {
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(
        absolute ? url : new URL(url, window.location.origin).toString()
      );
      toast.success(`Copied ${label}`);
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };
  const openHref = absolute ? url : resolveBundleUrl(url);
  return (
    <div className='flex items-center gap-2'>
      <div className='flex-1 space-y-0.5 overflow-hidden'>
        <div className='flex items-center gap-1.5'>
          <span className='text-muted-foreground text-xs'>{label}</span>
          {recommended && (
            <span className='rounded bg-primary/10 px-1.5 py-0.5 font-medium text-[10px] text-primary uppercase tracking-wide'>
              Recommended
            </span>
          )}
        </div>
        <div className='truncate font-mono text-xs'>{url}</div>
      </div>
      <Button
        variant='ghost'
        size='icon'
        className='size-7 shrink-0'
        onClick={copy}
        aria-label={`Copy ${label}`}
      >
        <Copy className='size-3.5' />
      </Button>
      <Button
        variant='ghost'
        size='icon'
        className='size-7 shrink-0'
        onClick={() => window.open(openHref, "_blank", "noopener,noreferrer")}
        aria-label={`Open ${label} in a new tab`}
      >
        <ExternalLink className='size-3.5' />
      </Button>
    </div>
  );
};
