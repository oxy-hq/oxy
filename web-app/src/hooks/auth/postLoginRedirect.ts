import ROUTES from "@/libs/utils/routes";
import type { OrgInfo, UserInfo } from "@/types/auth";

export const PENDING_INVITE_TOKEN_KEY = "pending_invite_token";

export function handlePostLoginOrgs(user: UserInfo, orgs: OrgInfo[]): string {
  const pendingInviteToken = sessionStorage.getItem(PENDING_INVITE_TOKEN_KEY);
  if (pendingInviteToken) {
    sessionStorage.removeItem(PENDING_INVITE_TOKEN_KEY);
    return ROUTES.INVITE(pendingInviteToken);
  }

  if (user.is_owner) {
    return ROUTES.ADMIN.BILLING_QUEUE;
  }

  if (orgs.length === 0) {
    return ROUTES.ONBOARDING;
  }

  return ROUTES.ROOT;
}
