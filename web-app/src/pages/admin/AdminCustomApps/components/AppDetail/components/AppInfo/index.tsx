import { AlertCircle, ChevronRight, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppDebug } from "@/hooks/api/customApps/useCustomApps";
import { cn } from "@/libs/shadcn/utils";
import { resolveBundleUrl } from "@/pages/admin/AdminCustomApps/resolveBundleUrl";
import type { CustomApp } from "@/types/apps";
import { CopyButton } from "../../../AppsTable/components/UrlActions";

/**
 * Diagnostics dossier for the selected app: what oxy currently resolves (bundle
 * dir, manifest source, raw manifest) as a scannable health readout rather than
 * a wall of pills. Three LED health chips up top answer "is this app wired
 * right?" at a glance; URLs + identity are compact rows; the raw manifest is a
 * copyable, collapsed-by-default block so it stops dominating the panel.
 *
 * Read-only by design — Settings owns mutations. Project/branch come from the
 * admin row (`app`), not the bundle-public debug snapshot.
 */
export const AppInfo = ({ app }: { app: CustomApp }) => {
  const { data, isLoading, error } = useAppDebug(app.org_slug, app.slug);

  if (isLoading) {
    return (
      <div className='space-y-4 p-4'>
        <Skeleton className='h-20 w-full' />
        <Skeleton className='h-40 w-full' />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className='flex items-center gap-2 p-4 text-destructive text-sm'>
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
    <div className='space-y-5 p-4'>
      {/* Health readout — three LED chips the operator scans first. */}
      <div className='grid grid-cols-3 gap-2'>
        <HealthChip
          label='Bundle'
          ok={dirOk}
          value={isRemote ? "remote" : dirOk ? "exists" : "missing"}
          subtle={isRemote ? (data.upstream_url ?? "—") : (data.bundle_dir ?? "—")}
        />
        <HealthChip
          label='Manifest'
          ok={manifestOk}
          value={
            isRemote
              ? "external"
              : data.manifest_source === "db_override"
                ? "DB override"
                : "bundled"
          }
          subtle={
            isRemote
              ? "owned by upstream"
              : manifestOk
                ? "loaded"
                : (data.manifest_error ?? "error")
          }
        />
        <HealthChip
          label='Source'
          ok
          value={data.app.source_type}
          subtle={`${app.branch} branch`}
        />
      </div>

      {/* URLs — what to share with the customer or use in iframes. */}
      <Section title='URLs'>
        <UrlRow label='Subpath' url={app.url} />
        {app.url_subdomain && (
          <UrlRow label='Subdomain' url={app.url_subdomain} recommended absolute />
        )}
      </Section>

      {/* Identity */}
      <Section title='Identity'>
        <KV k='App ID' v={data.app.id} mono />
        <KV k='Project' v={app.project_id} mono />
        <KV k='Branch' v={app.branch} mono />
        <KV k='Status' v={data.app.status} />
      </Section>

      {/* Manifest error, if any — never for remote bundles (no oxy-side manifest). */}
      {!isRemote && data.manifest_error && (
        <Section title='Manifest error' tone='destructive'>
          <pre className='whitespace-pre-wrap rounded-md bg-destructive/10 p-3 text-destructive text-xs'>
            {data.manifest_error}
          </pre>
        </Section>
      )}

      {/* Raw manifest — collapsed by default so it stops dominating; copyable. */}
      {!isRemote && manifestOk && <ManifestBlock manifest={data.manifest} />}
    </div>
  );
};

/** A LED status chip: brand dot = healthy, destructive dot = broken. */
const HealthChip = ({
  label,
  ok,
  value,
  subtle
}: {
  label: string;
  ok: boolean;
  value: string;
  subtle?: string;
}) => (
  <div className='min-w-0 rounded-md border bg-card px-2.5 py-2'>
    <div className='flex items-center gap-1.5'>
      <span
        aria-hidden
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          ok ? "bg-primary ring-2 ring-primary/25" : "bg-destructive"
        )}
      />
      <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
        {label}
      </span>
    </div>
    <div className='mt-1 truncate font-medium text-xs'>{value}</div>
    {subtle && (
      <div className='truncate font-mono text-[10px] text-muted-foreground/80' title={subtle}>
        {subtle}
      </div>
    )}
  </div>
);

const ManifestBlock = ({ manifest }: { manifest: unknown }) => {
  const json = JSON.stringify(manifest, null, 2);
  return (
    <Section title='Manifest'>
      <Collapsible>
        <div className='flex items-center justify-between'>
          <CollapsibleTrigger className='group flex items-center gap-1 rounded text-muted-foreground text-xs hover:text-foreground'>
            <ChevronRight className='size-3 transition-transform group-data-[state=open]:rotate-90' />
            View raw manifest
          </CollapsibleTrigger>
          <CopyButton value={json} label='manifest' />
        </div>
        <CollapsibleContent>
          <pre className='mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded-md border bg-muted/40 p-3 font-mono text-[11px] text-foreground/90'>
            {json}
          </pre>
        </CollapsibleContent>
      </Collapsible>
    </Section>
  );
};

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
      className={cn(
        "mb-2 font-medium text-[10px] uppercase tracking-wider",
        tone === "destructive" ? "text-destructive" : "text-muted-foreground"
      )}
    >
      {title}
    </h3>
    <div className='space-y-1.5'>{children}</div>
  </div>
);

const KV = ({ k, v, mono }: { k: string; v: string; mono?: boolean }) => (
  <div className='flex items-baseline justify-between gap-4 border-b py-1.5 text-sm last:border-0'>
    <span className='shrink-0 text-muted-foreground'>{k}</span>
    <span className={cn("min-w-0 truncate text-right", mono && "font-mono text-xs")} title={v}>
      {v}
    </span>
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
  recommended?: boolean;
  absolute?: boolean;
}) => {
  const copyValue = absolute ? url : new URL(url, window.location.origin).toString();
  const openHref = absolute ? url : resolveBundleUrl(url);
  return (
    <div className='flex items-center gap-1.5'>
      <div className='min-w-0 flex-1 overflow-hidden'>
        <div className='flex items-center gap-1.5'>
          <span className='text-muted-foreground text-xs'>{label}</span>
          {recommended && (
            <span className='rounded bg-primary/10 px-1 py-0.5 font-medium text-[9px] text-primary uppercase tracking-wide'>
              Recommended
            </span>
          )}
        </div>
        <div className='truncate font-mono text-xs' title={url}>
          {url}
        </div>
      </div>
      <CopyButton value={copyValue} label={label} />
      <Button
        variant='ghost'
        size='icon'
        className='size-6 shrink-0'
        onClick={() => window.open(openHref, "_blank", "noopener,noreferrer")}
        aria-label={`Open ${label} in a new tab`}
      >
        <ExternalLink className='size-3.5' />
      </Button>
    </div>
  );
};
