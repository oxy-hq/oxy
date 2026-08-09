import { AppWindow, Building2, Handshake, ShieldCheck } from "lucide-react";
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
  | "app_operator"
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
  // Staff, but NOT the amber "reaches everything" shield — an App Operator ships
  // custom apps and holds nothing else, so giving it the Global Admin badge (which
  // is what a bare `is_app_admin` boolean produced) overstates it on every row.
  app_operator: {
    label: "App Operator",
    icon: AppWindow,
    cls: "border-sky-500/30 text-sky-600 dark:text-sky-400"
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

/**
 * Map a stored platform role (`PlatformRole::as_str`) to a badge kind.
 *
 * Every caller must go through this rather than deriving a badge from
 * `is_app_admin`. That boolean is true for App Operators too, so the old
 * `is_app_admin && <RoleBadge kind='global_admin' />` labelled an app publisher as
 * platform staff-with-everything on every row it appeared in.
 *
 * `null` for an unknown role: a grant whose role this build can't name should show
 * no badge rather than a confidently wrong one — the same fail-closed reading the
 * loader applies.
 */
export function platformRoleKind(role: string | null | undefined): RoleKind | null {
  if (role === "global_admin") return "global_admin";
  if (role === "app_operator") return "app_operator";
  return null;
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
