import { Building2, Handshake } from "lucide-react";
import RelationshipChart, { type RelationshipNodeData } from "@/components/graph/RelationshipChart";
import type { AdminPartnerDetail } from "@/types/adminPartners";

/**
 * The partner's org chart, rendered with the shared React Flow + ELK graph (same
 * engine as the World Model / Context Graph) — pan/zoom, auto-laid-out top-down.
 *
 * This chart is the **organizational** hierarchy only: the partner at the root, the
 * client orgs it manages below. Operators (people) are deliberately kept out — they
 * live in the People section, and mixing people with orgs at one tier reads as if an
 * operator were a client. Their count is summarized on the partner node instead.
 */
export default function PartnerOrgChart({ partner }: { partner: AdminPartnerDetail }) {
  const operatorCount = partner.people.filter((p) => p.has_access).length;
  const clientCount = partner.managed_orgs.length;

  const nodes: Array<{ id: string; data: RelationshipNodeData }> = [
    {
      id: "partner",
      data: {
        label: partner.name,
        sublabel: `${partner.slug} · ${operatorCount} operator${
          operatorCount === 1 ? "" : "s"
        } · ${clientCount} client${clientCount === 1 ? "" : "s"}`,
        tone: "root",
        icon: Handshake
      }
    },
    ...partner.managed_orgs.map((o) => ({
      id: `client-${o.org_id}`,
      data: {
        label: o.org_name ?? o.org_id,
        sublabel: o.org_slug ?? undefined,
        icon: Building2
      } satisfies RelationshipNodeData
    }))
  ];

  const edges = partner.managed_orgs.map((o) => ({
    source: "partner",
    target: `client-${o.org_id}`
  }));

  return <RelationshipChart nodes={nodes} edges={edges} height={360} />;
}
