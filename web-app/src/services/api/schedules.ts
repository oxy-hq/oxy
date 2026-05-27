import type {
  BackfillInput,
  BackfillResponse,
  RunNowResponse,
  Schedule,
  ScheduleInput
} from "@/types/schedule";
import { apiClient } from "./axios";

/** Client for the `/agentic-schedules` HTTP surface (mounted per workspace). */
export class ScheduleService {
  private static base(projectId: string): string {
    return `/${projectId}/agentic-schedules`;
  }

  static async list(projectId: string): Promise<Schedule[]> {
    const { data } = await apiClient.get(ScheduleService.base(projectId));
    return data;
  }

  static async get(projectId: string, id: string): Promise<Schedule> {
    const { data } = await apiClient.get(`${ScheduleService.base(projectId)}/${id}`);
    return data;
  }

  static async create(projectId: string, input: ScheduleInput): Promise<Schedule> {
    const { data } = await apiClient.post(ScheduleService.base(projectId), input);
    return data;
  }

  static async update(projectId: string, id: string, input: ScheduleInput): Promise<Schedule> {
    const { data } = await apiClient.patch(`${ScheduleService.base(projectId)}/${id}`, input);
    return data;
  }

  static async remove(projectId: string, id: string): Promise<void> {
    await apiClient.delete(`${ScheduleService.base(projectId)}/${id}`);
  }

  static async runNow(projectId: string, id: string): Promise<RunNowResponse> {
    const { data } = await apiClient.post(`${ScheduleService.base(projectId)}/${id}/run-now`);
    return data;
  }

  static async backfill(
    projectId: string,
    id: string,
    body: BackfillInput
  ): Promise<BackfillResponse> {
    const { data } = await apiClient.post(
      `${ScheduleService.base(projectId)}/${id}/backfill`,
      body
    );
    return data;
  }
}
