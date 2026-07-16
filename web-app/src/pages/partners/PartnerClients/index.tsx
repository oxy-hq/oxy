import { Building2 } from "lucide-react";
import { useState } from "react";
import { usePartnerOrgs } from "@/hooks/api/partners";
import { usePartnerConsole } from "../context";
import ClientPane from "./components/ClientPane";
import ClientRail from "./components/ClientRail";

/**
 * Your clients — the partner-scoped mirror of the admin Tenants directory. One
 * master-detail surface: a dense client rail beside a full detail pane (members,
 * apps, rename, act-as). Replaces the old two-page split (a client list, then a
 * separate members page behind an org picker) — everything is on the surface, one
 * level deep, scoped to the orgs this partner manages.
 */
export default function PartnerClients() {
  const { active } = usePartnerConsole();
  const partnerId = active.partner_id;
  const caps = active.capabilities;

  const { data: orgs, isLoading } = usePartnerOrgs(partnerId);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = orgs?.find((o) => o.org_id === selectedId) ?? orgs?.[0];

  return (
    <div className='flex h-full min-h-0' data-testid='partner-clients-page'>
      <aside className='flex w-72 shrink-0 flex-col border-r'>
        <ClientRail
          orgs={orgs}
          isLoading={isLoading}
          selectedId={selected?.org_id}
          onSelect={setSelectedId}
          partnerId={partnerId}
          canCreate={caps.create_orgs}
        />
      </aside>

      <main className='min-h-0 flex-1 overflow-y-auto'>
        {selected ? (
          <ClientPane key={selected.org_id} partnerId={partnerId} org={selected} caps={caps} />
        ) : (
          <div className='flex h-full flex-col items-center justify-center gap-2 text-muted-foreground'>
            <Building2 className='size-8' />
            <p className='text-sm'>
              {isLoading ? "Loading clients…" : "No clients yet — onboard one from the rail."}
            </p>
          </div>
        )}
      </main>
    </div>
  );
}
