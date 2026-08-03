import { Switch } from "@/components/ui/shadcn/switch";
import { useSetPartnerPersonAccess } from "@/hooks/api/adminPartners";
import { orgRoleKind, RoleBadge } from "@/pages/admin/components/RoleBadge";
import {
  type AdminPartnerCapabilities,
  type AdminPartnerDetail,
  CAPABILITY_LABELS
} from "@/types/adminPartners";
import { RowLine } from "./paneParts";

/**
 * The partner's people, with a **partner-access toggle** per person (staff
 * override — audited `via_global_override`).
 *
 * Access is all-or-nothing: an operator reaches every client the partner manages
 * and can do everything the **ceiling** allows. There are no per-person roles, so
 * the only decision here is in-or-out. Normally the partner's own owner/admin makes
 * it; staff can too, to bootstrap or repair a partnership.
 */
export default function PartnerPeopleList({ partner }: { partner: AdminPartnerDetail }) {
  const setAccess = useSetPartnerPersonAccess(partner.org_id);

  if (partner.people.length === 0) {
    return (
      <p className='text-muted-foreground text-xs'>
        {partner.name} has no members yet. Add people to the org, then grant them partner access
        here.
      </p>
    );
  }

  return (
    <div className='space-y-3'>
      <CeilingSummary ceiling={partner.capabilities} />
      <div className='space-y-2'>
        {partner.people.map((p) => (
          <RowLine
            key={p.org_member_id}
            primary={p.email}
            secondary={p.has_access ? "Partner operator" : "No partner access"}
            trailing={
              <div className='flex items-center gap-3'>
                <RoleBadge kind={orgRoleKind(p.org_role)} />
                <Switch
                  checked={p.has_access}
                  disabled={setAccess.isPending}
                  onCheckedChange={(hasAccess) =>
                    setAccess.mutate({ orgMemberId: p.org_member_id, hasAccess })
                  }
                  aria-label={`Partner access for ${p.email}`}
                />
              </div>
            }
          />
        ))}
      </div>
    </div>
  );
}

/** What "partner access" concretely grants — the ceiling, in words. */
function CeilingSummary({ ceiling }: { ceiling: AdminPartnerCapabilities }) {
  const granted = (Object.keys(CAPABILITY_LABELS) as (keyof AdminPartnerCapabilities)[]).filter(
    (cap) => ceiling[cap]
  );
  return (
    <p className='text-muted-foreground text-xs'>
      An operator here can:{" "}
      {granted.length === 0 ? (
        <span className='italic'>nothing — the ceiling grants no capabilities</span>
      ) : (
        <span className='text-foreground/80'>
          {granted.map((cap) => CAPABILITY_LABELS[cap]).join(" · ")}
        </span>
      )}
    </p>
  );
}
