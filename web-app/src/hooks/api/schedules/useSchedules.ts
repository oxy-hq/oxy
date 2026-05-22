/**
 * Hooks for the `/agentic-schedules` HTTP surface + the airway file
 * list used by the schedule target picker (workflow files reuse
 * `useAgenticWorkflowFiles`).
 */

import { type UseQueryResult, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { type AirwayFile, AirwayService } from "@/services/api/airway";
import { ScheduleService } from "@/services/api/schedules";
import type { Schedule, ScheduleInput } from "@/types/schedule";
import queryKeys from "../queryKey";

export const useSchedules = (): UseQueryResult<Schedule[]> => {
  const { project } = useCurrentProjectBranch();
  return useQuery({
    queryKey: queryKeys.schedule.list(project.id),
    queryFn: () => ScheduleService.list(project.id)
  });
};

export const useAirwayFiles = (): UseQueryResult<AirwayFile[]> => {
  const { project } = useCurrentProjectBranch();
  return useQuery({
    queryKey: queryKeys.airway.files(project.id),
    queryFn: () => AirwayService.listFiles(project.id)
  });
};

export const useCreateSchedule = () => {
  const { project } = useCurrentProjectBranch();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: ScheduleInput) => ScheduleService.create(project.id, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.schedule.list(project.id) });
      toast.success("Schedule created");
    },
    onError: (e: Error) => toast.error(`Failed to create schedule: ${e.message}`)
  });
};

export const useUpdateSchedule = () => {
  const { project } = useCurrentProjectBranch();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: ScheduleInput }) =>
      ScheduleService.update(project.id, id, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.schedule.list(project.id) });
      toast.success("Schedule updated");
    },
    onError: (e: Error) => toast.error(`Failed to update schedule: ${e.message}`)
  });
};

export const useDeleteSchedule = () => {
  const { project } = useCurrentProjectBranch();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ScheduleService.remove(project.id, id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.schedule.list(project.id) });
      toast.success("Schedule deleted");
    },
    onError: (e: Error) => toast.error(`Failed to delete schedule: ${e.message}`)
  });
};

export const useRunScheduleNow = () => {
  const { project } = useCurrentProjectBranch();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ScheduleService.runNow(project.id, id),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: queryKeys.schedule.list(project.id) });
      toast.success(`Run started (${res.run_id.slice(0, 8)}…)`);
    },
    onError: (e: Error) => toast.error(`Failed to run schedule: ${e.message}`)
  });
};
