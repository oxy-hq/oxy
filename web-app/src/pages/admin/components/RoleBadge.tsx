import { Building2, Handshake, ShieldCheck } from "lucide-react";
import type { ComponentType } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { cn } from "@/libs/shadcn/utils";

/**
 * One badge vocabulary for every "admin" in the product, scope-prefixed so the
 * word is never bare — the three sources of authority read distinctly:
 *   - global_*  (platform staff, everywhere)      → shield, amber
 *   - org_*     (a person↔org role)               → building, neutral
 *   - partner_* (delegated via a partner grant)   → handshake, brand/primary
 * Shared so users list / dossier / org members / partner members never drift.
 */
export type RoleKind =
  | "global_owner"
  | "global_admin"
  | "org_owner"
  | "org_admin"
  | "org_member"
  | "partner_operator";

const ROLE: Record<
  RoleKind,
  { label: string; icon: ComponentType<{ className?: string }>; cls: string }
> = {
  global_owner: {
    label: "Global Owner",
    icon: ShieldCheck,
    cls: "border-amber-500/30 text-amber-600 dark:text-amber-400"
  },
  global_admin: {
    label: "Global Admin",
    icon: ShieldCheck,
    cls: "border-amber-500/30 text-amber-600 dark:text-amber-400"
  },
  org_owner: { label: "Org Owner", icon: Building2, cls: "border-border text-foreground" },
  org_admin: { label: "Org Admin", icon: Building2, cls: "border-border text-muted-foreground" },
  org_member: { label: "Member", icon: Building2, cls: "border-border text-muted-foreground" },
  // Partner authority: the handshake + brand colour marks it as delegated via a
  // partner grant. There is one operator role, so one badge.
  partner_operator: {
    label: "Partner access",
    icon: Handshake,
    cls: "border-primary/30 text-primary"
  }
};

/** Map a raw org role string ("owner" | "admin" | "member") to a badge kind. */
export function orgRoleKind(role: string): RoleKind {
  if (role === "owner") return "org_owner";
  if (role === "admin") return "org_admin";
  return "org_member";
}

export function RoleBadge({ kind, className }: { kind: RoleKind; className?: string }) {
  const { label, icon: Icon, cls } = ROLE[kind];
  return (
    <Badge
      variant='outline'
      className={cn("gap-1 px-1.5 py-0 font-medium text-[10px]", cls, className)}
    >
      <Icon className='size-3' />
      {label}
    </Badge>
  );
}
