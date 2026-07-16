import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { PartnersService } from "@/services/api/partners";
import type { ChildOrg } from "@/types/partners";
import queryKeys from "../queryKey";

/** Partners the current user administers (drives the console entry). */
export const useMyPartners = () =>
  useQuery({
    queryKey: queryKeys.partner.mine(),
    queryFn: () => PartnersService.listMine(),
    retry: false
  });

/** Child orgs a partner manages. */
export const usePartnerOrgs = (partnerId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.orgs(partnerId ?? ""),
    queryFn: () => PartnersService.orgs(partnerId as string),
    enabled: !!partnerId
  });

/** Members of one partner-managed org. */
export const usePartnerOrgMembers = (partnerId: string | undefined, orgId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.members(partnerId ?? "", orgId ?? ""),
    queryFn: () => PartnersService.members(partnerId as string, orgId as string),
    enabled: !!partnerId && !!orgId
  });

/** Workspaces in one partner-managed org (read-only; gated by `manage_apps`). */
export const usePartnerOrgWorkspaces = (partnerId: string | undefined, orgId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.workspaces(partnerId ?? "", orgId ?? ""),
    queryFn: () => PartnersService.workspaces(partnerId as string, orgId as string),
    enabled: !!partnerId && !!orgId
  });

/** Health rollup across the partner's managed clients' workspaces (worst-first). */
export const usePartnerHealth = (partnerId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.health(partnerId ?? ""),
    queryFn: () => PartnersService.health(partnerId as string),
    enabled: !!partnerId
  });

/** App-scoped publish tokens for one client app (CI credentials). */
export const usePartnerAppTokens = (partnerId: string | undefined, appId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.appTokens(partnerId ?? "", appId ?? ""),
    queryFn: () => PartnersService.appTokens(partnerId as string, appId as string),
    enabled: !!partnerId && !!appId
  });

export const useCreateAppToken = (partnerId: string, appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name?: string) => PartnersService.createAppToken(partnerId, appId, name),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: queryKeys.partner.appTokens(partnerId, appId) }),
    onError: (e: unknown) =>
      toast.error(
        status(e) === 403
          ? "This client hasn't enabled partner publishing"
          : "Failed to create the token"
      )
  });
};

export const useRevokeAppToken = (partnerId: string, appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (tokenId: string) => PartnersService.revokeAppToken(partnerId, appId, tokenId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.partner.appTokens(partnerId, appId) });
      toast.success("Token revoked");
    },
    onError: () => toast.error("Failed to revoke the token")
  });
};

/** Partner-subtree audit log. */
export const usePartnerAudit = (partnerId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.partner.audit(partnerId ?? ""),
    queryFn: () => PartnersService.audit(partnerId as string),
    enabled: !!partnerId
  });

/** Custom apps in one partner-managed org (loaded lazily on row expand). */
export const usePartnerOrgApps = (
  partnerId: string | undefined,
  orgId: string | undefined,
  enabled = true
) =>
  useQuery({
    queryKey: queryKeys.partner.apps(partnerId ?? "", orgId ?? ""),
    queryFn: () => PartnersService.orgApps(partnerId as string, orgId as string),
    enabled: !!partnerId && !!orgId && enabled
  });

/** One managed app, tagged with the client org it belongs to. */
export type PartnerAppWithOrg = Awaited<ReturnType<typeof PartnersService.orgApps>>[number] & {
  orgId: string;
  orgName: string;
};

/**
 * Every managed app across ALL the partner's clients, for the top-level Custom
 * apps surface. Apps are a per-org resource, so this fans out one query per
 * client (a partner manages a handful) and flattens — each app tagged with its
 * client so the surface can show and act on it in one place.
 */
export const usePartnerApps = (partnerId: string | undefined, orgs: ChildOrg[] | undefined) => {
  const list = orgs ?? [];
  const results = useQueries({
    queries: list.map((o) => ({
      queryKey: queryKeys.partner.apps(partnerId ?? "", o.org_id),
      queryFn: () => PartnersService.orgApps(partnerId as string, o.org_id),
      enabled: !!partnerId
    }))
  });
  const apps: PartnerAppWithOrg[] = results.flatMap((r, i) =>
    (r.data ?? []).map((a) => ({ ...a, orgId: list[i].org_id, orgName: list[i].name }))
  );
  return {
    apps,
    isLoading: results.some((r) => r.isLoading),
    isError: results.some((r) => r.isError)
  };
};

/** Publish / unpublish a managed app (gated by the `manage_apps` capability). */
export const useSetAppPublished = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ appId, published }: { appId: string; orgId: string; published: boolean }) =>
      PartnersService.setAppPublished(partnerId, appId, published),
    onSuccess: (_data, { orgId }) => {
      qc.invalidateQueries({ queryKey: queryKeys.partner.apps(partnerId, orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.partner.orgs(partnerId) });
    }
  });
};

export const useInviteMember = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, email, role }: { orgId: string; email: string; role: string }) =>
      PartnersService.inviteMember(partnerId, orgId, { email, role }),
    onSuccess: (_data, { orgId }) => {
      qc.invalidateQueries({
        queryKey: queryKeys.partner.members(partnerId, orgId)
      });
    }
  });
};

export const useUpdateMemberRole = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, userId, role }: { orgId: string; userId: string; role: string }) =>
      PartnersService.updateMemberRole(partnerId, orgId, userId, role),
    onSuccess: (_data, { orgId }) => {
      qc.invalidateQueries({
        queryKey: queryKeys.partner.members(partnerId, orgId)
      });
    }
  });
};

export const useRemoveMember = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, userId }: { orgId: string; userId: string }) =>
      PartnersService.removeMember(partnerId, orgId, userId),
    onSuccess: (_data, { orgId }) => {
      qc.invalidateQueries({
        queryKey: queryKeys.partner.members(partnerId, orgId)
      });
    }
  });
};

// ── partner-initiated onboarding ───────────────────────────────────────────

/** Requires `create_orgs`. Creates the client AND attaches it, in one txn. */
export const useCreateClientOrg = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { name: string; slug: string; owner_email?: string }) =>
      PartnersService.createOrg(partnerId, body),
    onSuccess: (created) => {
      qc.invalidateQueries({ queryKey: queryKeys.partner.orgs(partnerId) });
      qc.invalidateQueries({ queryKey: queryKeys.partner.mine() });
      toast.success(
        created.owner_pending
          ? `${created.org.name} created — invite the owner from Members`
          : `${created.org.name} created`
      );
    },
    onError: (e: unknown) => {
      const code = status(e);
      if (code === 409) return toast.error("That slug is taken");
      if (code === 422) return toast.error("That slug is reserved");
      if (code === 403) return toast.error("Your role does not allow onboarding clients");
      toast.error("Failed to create the client");
    }
  });
};

/** Requires `manage_org_settings`. */
export const useUpdateClientOrg = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, name }: { orgId: string; name: string }) =>
      PartnersService.updateOrg(partnerId, orgId, { name }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.partner.orgs(partnerId) });
      toast.success("Client updated");
    },
    onError: () => toast.error("Failed to update the client")
  });
};

// ── the partner staffing itself (partner_admin only) ───────────────────────

export const usePartnerPeople = (partnerId: string | undefined, enabled = true) =>
  useQuery({
    queryKey: queryKeys.partner.people(partnerId ?? ""),
    queryFn: () => PartnersService.people(partnerId as string),
    enabled: !!partnerId && enabled
  });

/** Grant or revoke partner access for a member of the partner org. */
export const useSetPersonAccess = (partnerId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ orgMemberId, hasAccess }: { orgMemberId: string; hasAccess: boolean }) => {
      if (hasAccess) await PartnersService.grantAccess(partnerId, orgMemberId);
      else await PartnersService.revokeAccess(partnerId, orgMemberId);
    },
    onSuccess: (_data, { hasAccess }) => {
      qc.invalidateQueries({ queryKey: queryKeys.partner.people(partnerId) });
      toast.success(hasAccess ? "Partner access granted" : "Partner access revoked");
    },
    onError: (e: unknown) =>
      toast.error(
        status(e) === 403
          ? "You cannot change your own partner access"
          : "Failed to change partner access"
      )
  });
};

function status(e: unknown): number | undefined {
  return typeof e === "object" && e !== null && "response" in e
    ? (e as { response?: { status?: number } }).response?.status
    : undefined;
}
