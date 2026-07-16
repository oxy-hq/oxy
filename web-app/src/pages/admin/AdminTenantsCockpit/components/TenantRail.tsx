import { X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Input } from "@/components/ui/shadcn/input";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAdminPartners } from "@/hooks/api/adminPartners";
import { useAdminOrgsList, useAdminUsersList } from "@/hooks/api/adminTenants/index";
import { cn } from "@/libs/shadcn/utils";
import { RoleBadge } from "@/pages/admin/components/RoleBadge";
import type { TenantType } from "../useTenantSelection";
import PartnerChip from "./PartnerChip";

function useDebounced(value: string, ms = 200) {
  const [d, setD] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setD(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return d;
}

/** A row's data, mapped from whichever entity the rail is showing. */
interface RailItem {
  id: string;
  primary: string;
  secondary?: string;
  meta?: ReactNode;
}

export default function TenantRail({
  type,
  selectedId,
  onSelect,
  partnerFilter,
  partnerFilterName,
  onClearPartnerFilter,
  onPartnerChip
}: {
  type: TenantType;
  selectedId: string | null;
  onSelect: (id: string) => void;
  partnerFilter: string | null;
  partnerFilterName?: string;
  onClearPartnerFilter: () => void;
  onPartnerChip: (partnerId: string) => void;
}) {
  const [search, setSearch] = useState("");
  const q = useDebounced(search);

  return (
    <div className='flex h-full flex-col'>
      <div className='space-y-1.5 border-b p-2'>
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={`Search ${type}…`}
          className='h-8'
        />
        {partnerFilter && (
          <button
            type='button'
            onClick={onClearPartnerFilter}
            className='flex w-full items-center gap-1.5 rounded bg-primary/5 px-2 py-1 text-primary text-xs'
          >
            <span className='truncate'>Managed by {partnerFilterName ?? "partner"}</span>
            <X className='ml-auto size-3 shrink-0' />
          </button>
        )}
      </div>

      <div data-testid='tenant-rail-list' className='min-h-0 flex-1 overflow-auto p-1'>
        {type === "orgs" && (
          <OrgListSource
            q={q}
            partnerFilter={partnerFilter}
            selectedId={selectedId}
            onSelect={onSelect}
            onPartnerChip={onPartnerChip}
          />
        )}
        {type === "users" && <UserListSource q={q} selectedId={selectedId} onSelect={onSelect} />}
        {type === "partners" && (
          <PartnerListSource q={q} selectedId={selectedId} onSelect={onSelect} />
        )}
      </div>
    </div>
  );
}

/** The shared dense list: renders `items`, wiring single-select + click-to-open. */
function RailList({
  items,
  isPending,
  selectedId,
  onSelect
}: {
  items: RailItem[];
  isPending: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  if (isPending) return <ListSkeleton />;
  if (!items.length)
    return (
      <p data-testid='tenant-rail-empty' className='p-3 text-muted-foreground text-xs'>
        No matches.
      </p>
    );

  return (
    <>
      {items.map((item) => (
        <button
          key={item.id}
          type='button'
          data-testid='tenant-rail-row'
          onClick={() => onSelect(item.id)}
          className={cn(
            "flex w-full items-center gap-2 rounded px-1.5 py-1 text-left transition-colors",
            item.id === selectedId ? "bg-muted" : "hover:bg-muted/60"
          )}
        >
          <span className='min-w-0 flex-1'>
            <span className='block truncate font-medium text-[13px] leading-tight'>
              {item.primary}
            </span>
            {item.secondary && (
              <span className='block truncate text-[11px] text-muted-foreground leading-tight'>
                {item.secondary}
              </span>
            )}
          </span>
          {item.meta && <span className='shrink-0'>{item.meta}</span>}
        </button>
      ))}
    </>
  );
}

function ListSkeleton() {
  return (
    <div className='space-y-1 p-1'>
      {[0, 1, 2, 3, 4, 5].map((i) => (
        <Skeleton key={i} className='h-7 w-full' />
      ))}
    </div>
  );
}

function OrgListSource({
  q,
  partnerFilter,
  selectedId,
  onSelect,
  onPartnerChip
}: {
  q: string;
  partnerFilter: string | null;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onPartnerChip: (partnerId: string) => void;
}) {
  const { data, isPending } = useAdminOrgsList({ search: q || undefined });
  // Partners are a separate population with their own view, so every org here is a
  // CUSTOMER — independent, or managed by a partner.
  const rows = (data ?? [])
    .filter((o) => !o.is_partner)
    .filter((o) => !partnerFilter || o.partner?.id === partnerFilter);
  const items: RailItem[] = rows.map((o) => ({
    id: o.id,
    primary: o.name,
    secondary: o.slug,
    meta: o.partner ? (
      <PartnerChip
        name={o.partner.name}
        size='xs'
        onClick={() => o.partner && onPartnerChip(o.partner.id)}
      />
    ) : undefined
  }));
  // Partners are excluded above, so a search for one comes back empty and the org
  // looks deleted. It isn't — point at it under Partners rather than let the
  // operator conclude it's gone.
  const partnerMatches = q.trim() ? (data ?? []).filter((o) => o.is_partner) : [];

  return (
    <>
      <RailList items={items} isPending={isPending} selectedId={selectedId} onSelect={onSelect} />
      {partnerMatches.length > 0 && (
        <div className='border-t px-2 py-2'>
          <p className='px-1 pb-1 font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
            Also a partner
          </p>
          <div className='flex flex-wrap gap-1'>
            {partnerMatches.map((o) => (
              <PartnerChip key={o.id} name={o.name} size='xs' onClick={() => onPartnerChip(o.id)} />
            ))}
          </div>
        </div>
      )}
    </>
  );
}

function UserListSource({
  q,
  selectedId,
  onSelect
}: {
  q: string;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const { data, isPending } = useAdminUsersList({ search: q || undefined });
  const items: RailItem[] = (data ?? []).map((u) => ({
    id: u.id,
    primary: u.name || u.email,
    secondary: u.email,
    meta: u.is_app_admin ? <RoleBadge kind='global_admin' /> : undefined
  }));
  return (
    <RailList items={items} isPending={isPending} selectedId={selectedId} onSelect={onSelect} />
  );
}

function PartnerListSource({
  q,
  selectedId,
  onSelect
}: {
  q: string;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const { data, isPending } = useAdminPartners();
  const rows = (data ?? []).filter(
    (p) =>
      !q ||
      p.name.toLowerCase().includes(q.toLowerCase()) ||
      p.slug.toLowerCase().includes(q.toLowerCase())
  );
  const items: RailItem[] = rows.map((p) => ({
    id: p.org_id,
    primary: p.name,
    secondary: p.slug,
    meta:
      p.status !== "active" ? (
        <Badge variant='destructive' className='text-[10px]'>
          {p.status}
        </Badge>
      ) : undefined
  }));
  return (
    <RailList items={items} isPending={isPending} selectedId={selectedId} onSelect={onSelect} />
  );
}
