import type { UseQueryResult } from "@tanstack/react-query";
import { PolicyFields } from "@/pages/admin/AdminAirway/components/PolicyFields";
import { PolicyPreview } from "@/pages/admin/AdminAirway/PolicyPreview";
import type {
  AirwayContractPolicy,
  AirwayEnvironment,
  AirwayPolicyPreviewResponse
} from "@/services/api/airwayConfig";
import { WorkspacePicker } from "./WorkspacePicker";

interface AddOverrideFormProps {
  sourceKind: string;
  workspace: { id: string; name: string } | null;
  onWorkspaceChange: (workspace: { id: string; name: string }) => void;
  excludeWorkspaceIds: string[];
  draftPolicy: AirwayContractPolicy | null;
  draftEnv: AirwayEnvironment | null;
  onPolicyChange: (value: AirwayContractPolicy | null) => void;
  onEnvChange: (value: AirwayEnvironment | null) => void;
  previewOpen: boolean;
  onRequestPreview: () => void;
  onHidePreview: () => void;
  preview: UseQueryResult<AirwayPolicyPreviewResponse>;
}

/**
 * The dialog body: workspace picker, the sparse policy/environment selects,
 * and the (kind-level, not workspace-scoped) preview disclosure. Split out
 * of `AddOverrideDialog` purely for file-size — this holds no state of its
 * own, `AddOverrideDialog` owns all of it.
 */
export function AddOverrideForm({
  sourceKind,
  workspace,
  onWorkspaceChange,
  excludeWorkspaceIds,
  draftPolicy,
  draftEnv,
  onPolicyChange,
  onEnvChange,
  previewOpen,
  onRequestPreview,
  onHidePreview,
  preview
}: AddOverrideFormProps) {
  return (
    <div className='space-y-4'>
      <div>
        <span className='mb-1.5 block font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Workspace
        </span>
        <WorkspacePicker
          value={workspace}
          onChange={onWorkspaceChange}
          excludeIds={excludeWorkspaceIds}
        />
      </div>

      <PolicyFields
        idSuffix={`override-${sourceKind}`}
        draftPolicy={draftPolicy}
        draftEnv={draftEnv}
        onPolicyChange={onPolicyChange}
        onEnvChange={onEnvChange}
        inheritPolicyLabel="Inherit this kind's policy"
        inheritEnvLabel="Inherit this kind's environment"
        inheritPolicyHelp={`No override — this workspace follows ${sourceKind}'s global policy above.`}
      />

      <div>
        <p className='mb-1.5 text-muted-foreground text-xs'>
          This previews the policy this override will actually run under — a field left on "inherit"
          is scored as {sourceKind}'s global value, not as airway's default. It scans every
          workspace, though: the preview endpoint takes no workspace, so it isn't scoped to the one
          selected above.
        </p>
        <PolicyPreview
          sourceKind={sourceKind}
          open={previewOpen}
          onRequestPreview={onRequestPreview}
          onHide={onHidePreview}
          preview={preview}
        />
      </div>
    </div>
  );
}
