import type { ReactNode } from "react";

import { useRole } from "@/hooks/useRole";

interface CanProps {
  children: ReactNode;
  /** Rendered when the caller does not have the role. Defaults to nothing. */
  fallback?: ReactNode;
}

/**
 * Declarative role gates that mirror the backend role guards 1:1
 * (OrgOwner, OrgAdmin, WorkspaceAdmin, WorkspaceEditor). Using them as JSX
 * wrappers makes the permission intent self-documenting and grep-auditable:
 * `grep "<Can" src` lists every gated UI surface.
 *
 * Use `useRole()` directly for inline checks (e.g. disabling a button with
 * a tooltip); use these wrappers for hide/show of destructive UI.
 */
export function CanOrgOwner({ children, fallback = null }: CanProps) {
  const { is } = useRole();
  return <>{is.orgOwner ? children : fallback}</>;
}

export function CanOrgAdmin({ children, fallback = null }: CanProps) {
  const { is } = useRole();
  return <>{is.orgAdmin ? children : fallback}</>;
}

export function CanWorkspaceAdmin({ children, fallback = null }: CanProps) {
  const { is } = useRole();
  return <>{is.workspaceAdmin ? children : fallback}</>;
}

export function CanWorkspaceEditor({ children, fallback = null }: CanProps) {
  const { is } = useRole();
  return <>{is.workspaceEditor ? children : fallback}</>;
}
