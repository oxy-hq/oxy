/**
 * Upload a payment-details report into a pipeline's landing zone.
 *
 * A hook rather than a call inside the component, per the API-layer rule: the
 * component owns the batch's UI state (which file is where), and the mutation
 * owns the request.
 *
 * Deliberately **not** invalidating a query on success. Landing a report is not
 * running the pipeline — nothing the page currently shows changes until a run
 * reads the zone, and invalidating run history here would imply otherwise.
 */

import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { AirwayService, type UploadedReport } from "@/services/api/airway";

export type UploadReportInput = {
  projectId: string;
  pipelineRef: string;
  file: File;
  /**
   * Both or neither — the server refuses half a period, because taking one
   * half and guessing the other stamps a month nobody named. Omitted, the
   * server reads the period from the file name.
   */
  period?: { year: number; month: number };
};

export const useUploadReport = (): UseMutationResult<UploadedReport, Error, UploadReportInput> =>
  useMutation({
    mutationFn: ({ projectId, pipelineRef, file, period }: UploadReportInput) =>
      AirwayService.uploadReport(projectId, pipelineRef, file, period)
  });
