import { AppWindow, Check, ChevronsUpDown, Copy, Plus, Search } from "lucide-react";
import { type KeyboardEvent, useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "@/components/ui/shadcn/command";
import { Input } from "@/components/ui/shadcn/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../../resolveBundleUrl";

interface AppListProps {
  apps: CustomerApp[];
  isLoading: boolean;
  selectedKey: string | null;
  onSelect: (app: CustomerApp) => void;
  onCreate: () => void;
  /** True when the server has more pages beyond what's currently in `apps`. */
  hasMore?: boolean;
  /** True while a follow-up page request is in flight. */
  isLoadingMore?: boolean;
  /** Trigger the next-page fetch. Wired by the parent to the infinite-query hook. */
  onLoadMore?: () => void;
}

type SourceFilter = "all" | "v0" | "local" | "s3";

/**
 * Left rail.
 *
 * Header is a triple-input bar: free-text search, an org combobox
 * (typeahead — scales past hundreds of orgs without becoming a chip
 * wall), and a source dropdown. No popover, no active-filter-tag
 * row; the selected values live inline on the controls themselves.
 *
 * List body still groups by org with sticky headers + dense rows:
 *   line 1: app name + source badge
 *   line 2: URL chips (subpath always, subdomain when present) with copy buttons
 *   line 3: project-uuid-short · last-synced
 *
 * Selection is a 2px left accent + soft bg lift — reads as "this is
 * current" without competing with the detail pane.
 */
export const AppList = (props: AppListProps) => {
  const {
    apps,
    isLoading,
    selectedKey,
    onSelect,
    onCreate,
    hasMore = false,
    isLoadingMore = false,
    onLoadMore
  } = props;
  const [q, setQ] = useState("");
  const [orgFilter, setOrgFilter] = useState<string | null>(null);
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>("all");
  const [orgPickerOpen, setOrgPickerOpen] = useState(false);

  // Sorted org list with per-org app counts. Derived from the page
  // the client has so far — same data the user sees, never stale.
  // Scales fine to hundreds of orgs because Command's filter is
  // O(n × stringLen) in the input string and runs purely in-memory.
  const orgOptions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const a of apps) {
      counts.set(a.org_slug, (counts.get(a.org_slug) ?? 0) + 1);
    }
    return Array.from(counts.entries())
      .map(([org, count]) => ({ org, count }))
      .sort((a, b) => a.org.localeCompare(b.org));
  }, [apps]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return apps.filter((a) => {
      if (orgFilter && a.org_slug !== orgFilter) return false;
      if (sourceFilter !== "all" && a.source_type !== sourceFilter) return false;
      if (!needle) return true;
      return (
        a.name.toLowerCase().includes(needle) ||
        a.slug.toLowerCase().includes(needle) ||
        a.org_slug.toLowerCase().includes(needle) ||
        a.project_id.toLowerCase().includes(needle)
      );
    });
  }, [apps, q, orgFilter, sourceFilter]);

  const groups = useMemo(() => groupByOrg(filtered), [filtered]);

  return (
    <aside className='flex h-full min-h-0 flex-col border-r bg-muted/20'>
      <div className='flex flex-col gap-2 border-b bg-background/60 p-3 backdrop-blur'>
        {/* Triple-input row. Search takes the available width;
            Org + Source are compact triggers. Create button sits
            flush right so the primary action stays anchored. */}
        <div className='flex flex-wrap items-center gap-1.5'>
          {/* Search basis floors at 12rem so flex-wrap fires once the
              Input would shrink narrower than its placeholder + a few
              chars. Without the floor, flex-1 lets the Input squeeze
              into the combobox at ~1265px viewport (≈ 300px list pane). */}
          <div className='relative min-w-48 flex-1'>
            <Search className='absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground' />
            <Input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder='Filter…'
              className='h-8 pl-8'
            />
          </div>

          {/* Org combobox. Trigger shows the selected org slug (or
              "All orgs"); the popover content is a Command pane with
              typeahead, so picking from 500 orgs feels the same as
              picking from 5. Per-org counts on the right edge of
              each item help disambiguate sibling orgs. */}
          <Popover open={orgPickerOpen} onOpenChange={setOrgPickerOpen}>
            <PopoverTrigger asChild>
              <Button
                variant='outline'
                size='sm'
                role='combobox'
                aria-expanded={orgPickerOpen}
                className={cn(
                  "h-8 shrink-0 justify-between gap-1.5 px-2 font-normal",
                  // Constrain the trigger so a long org slug doesn't
                  // push the search box too narrow.
                  "max-w-[140px]",
                  orgFilter ? "text-foreground" : "text-muted-foreground"
                )}
              >
                <span className='truncate font-mono text-xs'>{orgFilter ?? "All orgs"}</span>
                <ChevronsUpDown className='size-3 shrink-0 opacity-50' />
              </Button>
            </PopoverTrigger>
            <PopoverContent align='end' className='w-64 p-0'>
              <Command>
                <CommandInput placeholder='Search orgs…' className='h-9' />
                <CommandList>
                  <CommandEmpty>No matching orgs.</CommandEmpty>
                  <CommandGroup>
                    <CommandItem
                      value='__all__'
                      onSelect={() => {
                        setOrgFilter(null);
                        setOrgPickerOpen(false);
                      }}
                    >
                      <Check
                        className={cn(
                          "mr-2 size-3.5",
                          orgFilter === null ? "opacity-100" : "opacity-0"
                        )}
                      />
                      <span className='font-medium'>All orgs</span>
                      <span className='ml-auto font-mono text-muted-foreground text-xs'>
                        {apps.length}
                      </span>
                    </CommandItem>
                  </CommandGroup>
                  <CommandGroup heading='Organizations'>
                    {orgOptions.map(({ org, count }) => (
                      <CommandItem
                        key={org}
                        value={org}
                        onSelect={() => {
                          setOrgFilter(org === orgFilter ? null : org);
                          setOrgPickerOpen(false);
                        }}
                      >
                        <Check
                          className={cn(
                            "mr-2 size-3.5",
                            orgFilter === org ? "opacity-100" : "opacity-0"
                          )}
                        />
                        <span className='truncate font-mono text-xs'>{org}</span>
                        <span className='ml-auto font-mono text-muted-foreground text-xs'>
                          {count}
                        </span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>

          {/* Source select. Only three values (v0/local/s3) plus
              "all", so a plain Select beats a Combobox here —
              typeahead would be busywork for a 4-row menu. */}
          <Select value={sourceFilter} onValueChange={(v) => setSourceFilter(v as SourceFilter)}>
            <SelectTrigger
              className={cn(
                "h-8 w-auto gap-1 border-border bg-background px-2 text-xs",
                sourceFilter !== "all" && "text-foreground"
              )}
              aria-label='Filter by source type'
            >
              <SelectValue placeholder='Source' />
            </SelectTrigger>
            <SelectContent align='end'>
              <SelectItem value='all'>Any source</SelectItem>
              <SelectItem value='s3'>S3</SelectItem>
              <SelectItem value='local'>Local</SelectItem>
              <SelectItem value='v0'>v0</SelectItem>
            </SelectContent>
          </Select>

          <Button
            size='icon'
            className='size-8 shrink-0'
            onClick={onCreate}
            aria-label='Create app'
          >
            <Plus className='size-4' />
          </Button>
        </div>

        <div className='flex items-center justify-between font-mono text-muted-foreground text-xs uppercase tracking-wider'>
          <span>
            {filtered.length === apps.length
              ? `${apps.length} apps`
              : `${filtered.length} / ${apps.length}`}
          </span>
        </div>
      </div>

      <div className='min-h-0 flex-1 overflow-auto py-1'>
        {isLoading && (
          <div className='flex items-center justify-center gap-2 py-10 text-muted-foreground text-sm'>
            <Spinner /> Loading…
          </div>
        )}

        {!isLoading && filtered.length === 0 && apps.length > 0 && (
          <div className='py-10 text-center text-muted-foreground text-sm'>No matches.</div>
        )}

        {!isLoading && apps.length === 0 && (
          <div className='flex flex-col items-center gap-2 px-6 py-10 text-center'>
            <AppWindow className='size-6 text-muted-foreground/60' />
            <p className='text-muted-foreground text-sm'>No custom apps yet.</p>
            <Button size='sm' variant='outline' onClick={onCreate}>
              <Plus className='size-3.5' />
              Create the first
            </Button>
          </div>
        )}

        {groups.map(({ org, items }) => (
          <div key={org} className='mb-2'>
            <div className='sticky top-0 z-10 flex items-center justify-between gap-2 bg-muted/40 px-3 py-1 font-mono text-muted-foreground text-xs uppercase tracking-wider backdrop-blur'>
              <span className='truncate'>{org}</span>
              <span>{items.length}</span>
            </div>
            <ul className='space-y-px py-1'>
              {items.map((app) => (
                <AppRow
                  key={app.id}
                  app={app}
                  active={`${app.org_slug}/${app.slug}` === selectedKey}
                  onSelect={onSelect}
                />
              ))}
            </ul>
          </div>
        ))}

        {/* Pagination footer. Only the FILTERED count changes in
            response to the search box; "Load more" walks the server
            page cursor, so a hidden filter never silently swallows
            results that would have matched on a later page. */}
        {hasMore && onLoadMore && (
          <div className='flex items-center justify-center px-3 py-3'>
            <Button
              variant='outline'
              size='sm'
              className='h-7'
              disabled={isLoadingMore}
              onClick={onLoadMore}
            >
              {isLoadingMore ? (
                <>
                  <Spinner className='size-3.5' />
                  Loading…
                </>
              ) : (
                "Load more"
              )}
            </Button>
          </div>
        )}
      </div>
    </aside>
  );
};

/**
 * Renders the two URL forms (subdomain when present, subpath always)
 * as click-to-copy stubs the operator can grab without opening the
 * detail pane. Subpath shows the slug pair `<org>/<slug>`; subdomain
 * shows the host prefix `<org>--<slug>` so the row stays compact
 * while still being self-evident — the full URL goes on hover via the
 * `title` attr for cases where you need to paste the absolute form.
 *
 * Copy buttons stop event propagation so clicking them doesn't also
 * select the row (which would scroll the detail pane unexpectedly).
 */
const UrlChip = ({ label, short, full }: { label: string; short: string; full: string }) => (
  <span className='inline-flex items-center gap-1 truncate' title={`${label}: ${full}`}>
    <span className='shrink-0 text-muted-foreground/50'>{label}</span>
    <span className='truncate'>{short}</span>
    <button
      type='button'
      aria-label={`Copy ${label} URL`}
      className='inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/60 hover:bg-accent hover:text-foreground'
      onClick={async (e) => {
        e.stopPropagation();
        try {
          await navigator.clipboard.writeText(full);
          toast.success(`Copied ${label} URL`);
        } catch {
          toast.error("Failed to copy to clipboard");
        }
      }}
    >
      <Copy className='size-3' />
    </button>
  </span>
);

const AppRow = ({
  app,
  active,
  onSelect
}: {
  app: CustomerApp;
  active: boolean;
  onSelect: (a: CustomerApp) => void;
}) => {
  // Absolute URLs the operator can paste. `resolveBundleUrl` is the
  // shared helper used everywhere else (DetailToolbar, AppInfo,
  // CreateCustomerAppDialog) — it handles both the relative form
  // (`/customer-apps/...` resolved against window.location) and the
  // absolute form (rewrites host/port to the current admin origin so a
  // baked-in `:5173` from an older server doesn't sneak through).
  // Reinventing it as `${origin}${app.url}` would double-prefix any
  // absolute value into garbage like `https://app.oxygen-hq.comhttp://...`.
  const subpathAbs = resolveBundleUrl(app.url);
  const subpathShort = `${app.org_slug}/${app.slug}`;
  const subdomainAbs = app.url_subdomain ?? null;
  // Strip scheme + trailing slash for the compact label so it fits
  // alongside the subpath chip without wrapping. e.g.
  // "mars--command-center.customer-apps-dev.oxygen-hq.com"
  const subdomainShort = subdomainAbs
    ? subdomainAbs.replace(/^https?:\/\//, "").replace(/\/$/, "")
    : null;

  // Outer container intentionally renders as a div-with-button-role,
  // NOT an actual <button>. The URL chips inside are interactive
  // (Copy buttons) — nesting a <button> inside another <button> is
  // invalid HTML, triggers React's `validateDOMNesting` warning, and
  // produces implementation-defined hit-testing across browsers. The
  // div + role + tabIndex + Enter/Space handler reproduces the
  // keyboard-accessible click semantics of <button> without the
  // nesting violation.
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(app);
    }
  };

  return (
    <li>
      {/* biome-ignore lint/a11y/useSemanticElements: cannot use a real
           <button> here — UrlChip's copy <button> is rendered inside,
           and button-in-button is invalid HTML (see review on PR #2463
           and the long comment on `const onKey` above). */}
      <div
        role='button'
        tabIndex={0}
        onClick={() => onSelect(app)}
        onKeyDown={onKey}
        className={cn(
          "relative flex w-full cursor-pointer flex-col items-start gap-1 px-3 py-2 text-left outline-none transition-colors",
          "hover:bg-accent/60 focus-visible:bg-accent/60 focus-visible:ring-2 focus-visible:ring-ring",
          active && "bg-accent/80"
        )}
      >
        {active && (
          <span className='absolute inset-y-1.5 left-0 w-0.5 rounded-r-full bg-foreground' />
        )}

        <div className='flex w-full items-center gap-2'>
          <span
            className={cn(
              "max-w-full flex-1 truncate text-sm",
              active ? "font-medium text-foreground" : "font-normal text-foreground/90"
            )}
          >
            {app.name}
          </span>
          <Badge
            variant='outline'
            className='shrink-0 px-1.5 py-0 font-mono text-[9px] tracking-wide'
          >
            {app.source_type.toUpperCase()}
          </Badge>
        </div>

        {/* URLs row — what the operator most often needs to grab.
            Subpath always renders; subdomain only when the server
            surfaced it (v0 source + recognized admin host).
            `subdomainShort` is non-null iff `subdomainAbs` is, so a
            single guard on `subdomainAbs` suffices. */}
        <div className='flex w-full flex-wrap items-center gap-x-2 gap-y-0.5 font-mono text-[11px] text-muted-foreground'>
          <UrlChip label='path' short={subpathShort} full={subpathAbs} />
          {subdomainAbs && (
            <UrlChip label='sub' short={subdomainShort as string} full={subdomainAbs} />
          )}
        </div>

        <div className='flex w-full items-center gap-1.5 font-mono text-[11px] text-muted-foreground/70'>
          <span className='truncate'>proj:{app.project_id.slice(0, 8)}</span>
          {/* Right-edge stamp. Prefers `last_active_at` (someone opened
              the app recently) over `last_synced_at` (registry mutation
              time) because "is this thing being used?" is the higher-
              signal answer when triaging "what's stale?" — sync time
              only moves when an admin edits the row, which doesn't
              correlate with actual usage. Falls back to sync time when
              tracking has no rows yet (e.g. a freshly registered app). */}
          <span className='ml-auto shrink-0'>
            {app.last_active_at
              ? `active ${relativeTime(app.last_active_at)}`
              : app.last_synced_at
                ? relativeTime(app.last_synced_at)
                : "—"}
          </span>
        </div>
      </div>
    </li>
  );
};

function groupByOrg(apps: CustomerApp[]): Array<{ org: string; items: CustomerApp[] }> {
  const map = new Map<string, CustomerApp[]>();
  for (const a of apps) {
    const list = map.get(a.org_slug) ?? [];
    list.push(a);
    map.set(a.org_slug, list);
  }
  // Sort items within each org by updated_at DESC so the freshest
  // thing is on top of every group. Then sort the groups themselves
  // by their most-recent item, so the org that's been touched most
  // recently sits at the top of the rail. Falls back to alphabetical
  // when timestamps are equal (or missing).
  const groups = Array.from(map.entries()).map(([org, items]) => ({
    org,
    items: items.sort((a, b) => recencyScore(b) - recencyScore(a))
  }));
  groups.sort((a, b) => {
    const aMax = a.items[0] ? recencyScore(a.items[0]) : 0;
    const bMax = b.items[0] ? recencyScore(b.items[0]) : 0;
    if (aMax === bMax) return a.org.localeCompare(b.org);
    return bMax - aMax;
  });
  return groups;
}

/**
 * Numeric "how recent" score for an app. Prefers `updated_at` (set
 * on every metadata change) and falls back to `created_at`. Returns
 * a millisecond epoch; 0 on parse failure so a bad row sinks to the
 * bottom instead of throwing.
 */
function recencyScore(a: CustomerApp): number {
  const candidate = a.updated_at ?? a.created_at;
  if (!candidate) return 0;
  const t = new Date(candidate).getTime();
  return Number.isFinite(t) ? t : 0;
}

/** Compact "2m / 3h / 4d" style timestamp. */
function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const diff = Date.now() - then;
  const sec = Math.round(diff / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d`;
  const mo = Math.round(day / 30);
  return `${mo}mo`;
}
