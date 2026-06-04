import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type DomainPackBody } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * PUT the workspace's pack body. The backend renders every role's
 * template against a dummy context at PUT time; a typo or undefined
 * variable returns 400, which surfaces through the mutation's
 * `error` field so the editor can highlight the failing role.
 *
 * On success: invalidate the active pack so the role picker and
 * editor both pick up the new body without a full reload.
 */
const useUpdatePackBody = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ packKey, body }: { packKey: string; body: DomainPackBody }) => {
      return CameraService.updatePackBody(effectiveWorkspaceId, packKey, body);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.activePack(effectiveWorkspaceId)
      });
    }
  });
};

export default useUpdatePackBody;
