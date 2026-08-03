import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AppAccessService, TeamService } from "@/services/api/appAccess";
import { OrganizationService } from "@/services/api/organizations";
import type { AppAccess, GrantablePerson, SetAppAccessRequest, Team } from "@/types/appAccess";
import queryKeys from "../queryKey";

/**
 * Which surface is asking.
 *
 * The three consoles edit the same data through three different gates — org routes
 * need a real membership or a live assume-role session, `/admin/*` is closed while
 * an operator is acting, and the partner console is capability-scoped. The dialog
 * shouldn't care, so the scope is a value it passes through.
 */
export type AccessScope =
  | { kind: "org"; orgId: string }
  | { kind: "admin" }
  | { kind: "partner"; partnerId: string; orgId: string }
  // The partner acting on its OWN org's apps. Distinct from `partner` because the
  // routes and the authority differ: org authority, not the partner ceiling.
  | { kind: "partner-own"; partnerId: string };

/** A stable cache discriminator, so two surfaces never share an entry. */
const scopeKey = (scope: AccessScope): string => {
  switch (scope.kind) {
    case "org":
      return `org:${scope.orgId}`;
    case "admin":
      return "admin";
    case "partner":
      return `partner:${scope.partnerId}:${scope.orgId}`;
    case "partner-own":
      return `partner-own:${scope.partnerId}`;
  }
};

export const useAppAccess = (scope: AccessScope, appId: string | null) =>
  useQuery({
    queryKey: queryKeys.appAccess.detail(scopeKey(scope), appId ?? ""),
    queryFn: (): Promise<AppAccess> => {
      const id = appId as string;
      switch (scope.kind) {
        case "org":
          return AppAccessService.getForOrg(scope.orgId, id);
        case "admin":
          return AppAccessService.getForAdmin(id);
        case "partner":
        case "partner-own":
          return AppAccessService.getForPartner(scope.partnerId, id);
      }
    },
    enabled: !!appId,
    // The dialog seeds its editable state from this query, so a background refetch
    // would replace an admin's in-progress edits with the server copy mid-sentence.
    // These two stop that.
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    // But NOT `staleTime: Infinity`, which would trade the stomp for a worse bug:
    // the query would never refetch on re-enable, so reopening an app someone else
    // edited in the meantime would seed from the stale cache — and because the
    // endpoint is a full replace with no version check, saving would silently drop
    // their change.
    //
    // `gcTime: 0` is what actually makes reopening safe. `refetchOnMount: "always"`
    // alone does NOT: on reopen within the default gcTime, TanStack returns the
    // cached entry synchronously, so `isPending` is false, the dialog renders, and
    // the seed-once guard in the dialog fires on the STALE copy — then skips the
    // fresh response when it lands, because the guard has already recorded that app.
    // The two "fixes" cancelled out. With no cache to return, a reopen is always
    // cold, `isPending` gates the render, and the guard only ever sees fresh data.
    gcTime: 0,
    refetchOnMount: "always"
  });

export const useSetAppAccess = (scope: AccessScope) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ appId, body }: { appId: string; body: SetAppAccessRequest }) => {
      switch (scope.kind) {
        case "org":
          return AppAccessService.setForOrg(scope.orgId, appId, body);
        case "admin":
          return AppAccessService.setForAdmin(appId, body);
        case "partner":
        case "partner-own":
          return AppAccessService.setForPartner(scope.partnerId, appId, body);
      }
    },
    onSuccess: (data, vars) => {
      queryClient.setQueryData(queryKeys.appAccess.detail(scopeKey(scope), vars.appId), data);
      // The org settings list shows a visibility badge and grant count per app.
      queryClient.invalidateQueries({ queryKey: queryKeys.appAccess.orgAppsAll() });
      // Restricting an app changes which cards render on the launcher.
      queryClient.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    }
  });
};

/**
 * The teams this scope can grant.
 *
 * `enabled` is driven by the caller passing a non-null `appId` only while the dialog
 * is open — otherwise `/admin/apps/{id}/teams` fires on every dossier render and the
 * org teams list fires whenever the settings pane mounts, for a picker nobody is
 * looking at.
 */
export const useGrantableTeams = (scope: AccessScope, appId: string | null) =>
  useQuery({
    queryKey: queryKeys.appAccess.grantableTeams(scopeKey(scope), appId ?? ""),
    queryFn: (): Promise<Team[]> => {
      switch (scope.kind) {
        case "org":
          return TeamService.list(scope.orgId);
        case "admin":
          return AppAccessService.listTeamsForAdmin(appId as string);
        case "partner":
          return AppAccessService.listTeamsForPartner(scope.partnerId, scope.orgId);
        case "partner-own":
          return AppAccessService.listOwnTeams(scope.partnerId);
      }
    },
    enabled: !!appId
  });

/**
 * The people this scope can grant.
 *
 * The partner branch uses the `manage_apps`-gated `grantable-people` route, not the
 * `manage_members`-gated org member list — a partner managing a client's apps is not
 * expected to hold `manage_members`, and routing through it returned a silent 403
 * that emptied the picker and turned existing individual grants into "Unknown
 * person".
 */
export const useGrantablePeople = (scope: AccessScope, appId: string | null) =>
  useQuery({
    queryKey: queryKeys.appAccess.grantablePeople(scopeKey(scope), appId ?? ""),
    queryFn: async (): Promise<GrantablePerson[]> => {
      switch (scope.kind) {
        case "org": {
          const members = await OrganizationService.listMembers(scope.orgId);
          return members.map((m) => ({
            user_id: m.user_id,
            email: m.email,
            name: m.name || m.email,
            role: m.role
          }));
        }
        case "admin":
          return AppAccessService.listMembersForAdmin(appId as string);
        case "partner":
          return AppAccessService.listPeopleForPartner(scope.partnerId, scope.orgId);
        case "partner-own":
          return AppAccessService.listOwnPeople(scope.partnerId);
      }
    },
    enabled: !!appId
  });

/** The org's apps with their visibility — the App access settings list. */
export const useOrgAppAccessList = (orgId: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.appAccess.orgApps(orgId),
    queryFn: () => AppAccessService.listOrgApps(orgId),
    enabled: enabled && !!orgId
  });

/** The partner org's OWN apps, with visibility — the "Your apps" console panel. */
export const usePartnerOwnApps = (partnerId: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.appAccess.orgApps(`partner-own:${partnerId}`),
    queryFn: () => AppAccessService.listOwnApps(partnerId),
    enabled: enabled && !!partnerId
  });
