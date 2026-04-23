import ROUTES from "@/libs/utils/routes";
import type { OrgInfo } from "@/types/auth";

export const PENDING_INVITE_TOKEN_KEY = "pending_invite_token";

export function handlePostLoginOrgs(orgs: OrgInfo[]): string {
  const pendingInviteToken = sessionStorage.getItem(PENDING_INVITE_TOKEN_KEY);
  if (pendingInviteToken) {
    sessionStorage.removeItem(PENDING_INVITE_TOKEN_KEY);
    return ROUTES.INVITE(pendingInviteToken);
  }

  if (orgs.length === 0) {
    return ROUTES.ONBOARDING;
  }

  return ROUTES.ROOT;
}
