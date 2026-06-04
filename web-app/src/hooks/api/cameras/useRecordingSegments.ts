import { useQuery } from "@tanstack/react-query";
import { CameraService, type RecordingSegment } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Recording inventory for one camera — which time windows MTX still
 * has footage on disk for. Drives the scrubber's shaded bands and
 * the bare-scrub snap-to-segment behavior.
 *
 * Polled every minute so the inventory stays roughly in sync as the
 * edge box writes new segments and `recordDeleteAfter` rolls old
 * ones off. Cheap call (MTX `/list` is metadata-only).
 */
const useRecordingSegments = (
  workspaceId: string | undefined,
  cameraId: string | undefined,
  enabled = true
) =>
  useQuery<RecordingSegment[]>({
    queryKey: queryKeys.camera.recordingSegments(workspaceId ?? "", cameraId ?? ""),
    queryFn: () => CameraService.listRecordingSegments(workspaceId ?? "", cameraId ?? ""),
    enabled: enabled && !!workspaceId && !!cameraId,
    refetchInterval: 60_000
  });

export default useRecordingSegments;
