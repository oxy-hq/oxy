import { type QueryKey, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { OperatingGraphService } from "@/services/api/operatingGraph";
import type {
  AssignmentsFilter,
  CreateAssignmentRequest,
  CreateLocationRequest,
  CreateRoleRequest,
  PersonKind,
  UpdateLocationRequest
} from "@/types/operatingGraph";
import queryKeys from "../queryKey";
import { useFrontlineWorkers } from "./useFrontline";
import { useOrgMembers } from "./useOrgMembers";

/**
 * The operating graph: locations, positions and assignments.
 *
 * Every list is readable by any org member server-side, but the only surfaces
 * that read them are org-admin sections, so callers pass `enabled` from their
 * own `canManage` the way Crew does — a deep link must never fire reads on a
 * member's behalf for a panel the nav hides from them.
 */
export const useLocations = (orgId: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.org.locations(orgId),
    queryFn: async () => (await OperatingGraphService.listLocations(orgId)).locations,
    enabled: enabled && !!orgId
  });

export const useOrgRoles = (orgId: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.org.roles(orgId),
    queryFn: () => OperatingGraphService.listRoles(orgId),
    enabled: enabled && !!orgId
  });

export const useAssignments = (orgId: string, filter: AssignmentsFilter = {}, enabled = true) =>
  useQuery({
    queryKey: queryKeys.org.assignments(orgId, filter),
    queryFn: async () => (await OperatingGraphService.listAssignments(orgId, filter)).assignments,
    enabled: enabled && !!orgId
  });

/** Someone an assignment can name: an org member, or a frontline worker. */
export interface PersonOption {
  id: string;
  name: string;
  kind: PersonKind;
  /** What tells two people with the same name apart: an email, or a kiosk identifier. */
  detail: string;
}

/**
 * Everyone who has standing to hold a position, members first then crew,
 * each group by name. Two reads, one list — the pickers don't care which
 * table a person came from, only that the server will accept them.
 */
const byName = (a: PersonOption, b: PersonOption) => a.name.localeCompare(b.name);

export const usePeople = (orgId: string, enabled = true) => {
  const members = useOrgMembers(orgId, enabled);
  const workers = useFrontlineWorkers(orgId, enabled);
  const people = useMemo<PersonOption[]>(
    () => [
      ...(members.data ?? [])
        .map<PersonOption>((m) => ({
          id: m.user_id,
          name: m.name,
          kind: "member",
          detail: m.email
        }))
        .sort(byName),
      ...(workers.data ?? [])
        .filter((w) => w.status === "active")
        .map<PersonOption>((w) => ({
          id: w.user_id,
          name: w.name,
          kind: "frontline",
          detail: w.identifier
        }))
        .sort(byName)
    ],
    [members.data, workers.data]
  );
  return {
    people,
    isPending: members.isPending || workers.isPending,
    isError: members.isError || workers.isError
  };
};

/** One mutation shape for the whole graph: run, then invalidate every list the write can have changed. */
const useGraphMutation = <TVars extends { orgId: string }, TData>(
  mutationFn: (vars: TVars) => Promise<TData>,
  keysFor: (orgId: string) => QueryKey[]
) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: (_data, vars) => {
      for (const queryKey of keysFor(vars.orgId)) {
        queryClient.invalidateQueries({ queryKey });
      }
    }
  });
};

const locationKeys = (orgId: string) => [queryKeys.org.locations(orgId)];

// A location's name rides on assignment rows and kiosk rows, so a rename has
// to reach those lists too — not only the tree it was made in.
const locationNameKeys = (orgId: string) => [
  queryKeys.org.locations(orgId),
  queryKeys.org.assignments(orgId),
  queryKeys.org.frontlineWorkers(orgId),
  queryKeys.org.frontlineDevices(orgId)
];

const roleKeys = (orgId: string) => [queryKeys.org.roles(orgId)];

const roleNameKeys = (orgId: string) => [
  queryKeys.org.roles(orgId),
  queryKeys.org.assignments(orgId),
  queryKeys.org.frontlineWorkers(orgId)
];

// A worker row carries its own `assignments[]`, so every assignment write
// invalidates the crew list as well as the assignment views.
const assignmentKeys = (orgId: string) => [
  queryKeys.org.assignments(orgId),
  queryKeys.org.frontlineWorkers(orgId)
];

export const useCreateLocation = () =>
  useGraphMutation(
    (vars: { orgId: string; request: CreateLocationRequest }) =>
      OperatingGraphService.createLocation(vars.orgId, vars.request),
    locationKeys
  );

export const useUpdateLocation = () =>
  useGraphMutation(
    (vars: { orgId: string; locationId: string; request: UpdateLocationRequest }) =>
      OperatingGraphService.updateLocation(vars.orgId, vars.locationId, vars.request),
    locationNameKeys
  );

export const useSetExternalId = () =>
  useGraphMutation(
    (vars: { orgId: string; locationId: string; system: string; externalId: string }) =>
      OperatingGraphService.setExternalId(
        vars.orgId,
        vars.locationId,
        vars.system,
        vars.externalId
      ),
    locationKeys
  );

export const useDeleteExternalId = () =>
  useGraphMutation(
    (vars: { orgId: string; locationId: string; system: string }) =>
      OperatingGraphService.deleteExternalId(vars.orgId, vars.locationId, vars.system),
    locationKeys
  );

export const useCreateRole = () =>
  useGraphMutation(
    (vars: { orgId: string; request: CreateRoleRequest }) =>
      OperatingGraphService.createRole(vars.orgId, vars.request),
    roleKeys
  );

export const useRenameRole = () =>
  useGraphMutation(
    (vars: { orgId: string; roleId: string; name: string }) =>
      OperatingGraphService.renameRole(vars.orgId, vars.roleId, vars.name),
    roleNameKeys
  );

export const useDeleteRole = () =>
  useGraphMutation(
    (vars: { orgId: string; roleId: string }) =>
      OperatingGraphService.deleteRole(vars.orgId, vars.roleId),
    roleKeys
  );

export const useCreateAssignment = () =>
  useGraphMutation(
    (vars: { orgId: string; request: CreateAssignmentRequest }) =>
      OperatingGraphService.createAssignment(vars.orgId, vars.request),
    assignmentKeys
  );

export const useDeleteAssignment = () =>
  useGraphMutation(
    (vars: { orgId: string; assignmentId: string }) =>
      OperatingGraphService.deleteAssignment(vars.orgId, vars.assignmentId),
    assignmentKeys
  );
