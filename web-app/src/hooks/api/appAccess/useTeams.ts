import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { TeamService } from "@/services/api/appAccess";
import queryKeys from "../queryKey";

export const useTeams = (orgId: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.org.teams(orgId),
    queryFn: () => TeamService.list(orgId),
    enabled: enabled && !!orgId
  });

export const useTeam = (orgId: string, teamId: string | null) =>
  useQuery({
    queryKey: queryKeys.org.team(orgId, teamId ?? ""),
    queryFn: () => TeamService.get(orgId, teamId as string),
    enabled: !!orgId && !!teamId
  });

/**
 * Every mutation invalidates the team LIST as well as the team itself: the list
 * carries member counts, so adding one person changes a number two screens away.
 */
const useTeamMutation = <TVars extends { orgId: string; teamId?: string }, TData>(
  mutationFn: (vars: TVars) => Promise<TData>
) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: (_data, vars) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.teams(vars.orgId) });
      if (vars.teamId) {
        queryClient.invalidateQueries({ queryKey: queryKeys.org.team(vars.orgId, vars.teamId) });
      }
      // A team's grants are part of app access, and deleting a team revokes them.
      queryClient.invalidateQueries({ queryKey: queryKeys.appAccess.all });
    }
  });
};

export const useCreateTeam = () =>
  useTeamMutation((vars: { orgId: string; name: string; description?: string | null }) =>
    TeamService.create(vars.orgId, { name: vars.name, description: vars.description })
  );

export const useUpdateTeam = () =>
  useTeamMutation(
    (vars: { orgId: string; teamId: string; name: string; description?: string | null }) =>
      TeamService.update(vars.orgId, vars.teamId, {
        name: vars.name,
        description: vars.description
      })
  );

export const useDeleteTeam = () =>
  useTeamMutation((vars: { orgId: string; teamId: string }) =>
    TeamService.remove(vars.orgId, vars.teamId)
  );

export const useAddTeamMember = () =>
  useTeamMutation((vars: { orgId: string; teamId: string; userId: string }) =>
    TeamService.addMember(vars.orgId, vars.teamId, vars.userId)
  );

export const useRemoveTeamMember = () =>
  useTeamMutation((vars: { orgId: string; teamId: string; userId: string }) =>
    TeamService.removeMember(vars.orgId, vars.teamId, vars.userId)
  );
