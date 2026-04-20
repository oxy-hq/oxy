import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { OrgInfo } from "@/types/auth";

export const PENDING_INVITE_TOKEN_KEY = "pending_invite_token";

/**
 * Determines where to navigate after successful login based on org membership.
 * - pending invite in sessionStorage → resume the /invite/<token> flow
 * - 0 orgs → org list (ROOT) where user can create/join
 * - persisted org still valid → that org's workspace list
 * - 1 org → set it, navigate to its workspace list
 * - multiple orgs → org list to pick
 */
export function handlePostLoginOrgs(orgs: OrgInfo[]): string {
  const pendingInviteToken = sessionStorage.getItem(PENDING_INVITE_TOKEN_KEY);
  if (pendingInviteToken) {
    sessionStorage.removeItem(PENDING_INVITE_TOKEN_KEY);
    return ROUTES.INVITE(pendingInviteToken);
  }

  if (orgs.length === 0) {
    return ROUTES.ROOT;
  }

  const currentOrg = useCurrentOrg.getState().org;
  const stillMember = currentOrg && orgs.find((o) => o.id === currentOrg.id);

  if (stillMember) {
    return ROUTES.ORG(stillMember.slug).WORKSPACES;
  }

  const first = orgs[0];
  useCurrentOrg.getState().setOrg({
    id: first.id,
    name: first.name,
    slug: first.slug,
    role: first.role as "owner" | "admin" | "member"
  });

  if (orgs.length === 1) {
    return ROUTES.ORG(first.slug).WORKSPACES;
  }

  return ROUTES.ROOT;
}
