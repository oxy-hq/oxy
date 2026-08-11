import { RotateCcw } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { usePolicyPreview } from "@/hooks/api/airwayConfig/usePolicyPreview";
import {
  useDeleteAirwayGlobalConfig,
  useUpsertAirwayGlobalConfig
} from "@/hooks/api/airwayConfig/useUpsertAirwayConfig";
import {
  type AirwayContractPolicy,
  type AirwayEnvironment,
  type AirwaySourceKindConfig,
  asContractPolicy,
  asEnvironment
} from "@/services/api/airwayConfig";
import { PolicyFields } from "../components/PolicyFields";
import { SaveConfirmDialog } from "../components/SaveConfirmDialog";
import { PolicyPreview } from "../PolicyPreview";
import { computeSaveGate, formatUpdatedAt } from "../utils";
import { WorkspaceOverrides } from "../WorkspaceOverrides";

/**
 * One source kind's admission policy: the global row's two editable fields
 * (`contract_policy`, `environment`), a lazy preview disclosure, and its
 * per-workspace overrides. Changing either select never saves by itself —
 * it only arms the Save button and drops any preview already on screen, so
 * an operator can never save policy B while looking at a preview of policy A.
 */
export function SourceKindCard({ kind }: { kind: AirwaySourceKindConfig }) {
  // Narrowed at the boundary: the column is free text (no CHECK — see the
  // migration), so a row written by raw SQL or by a build that knew a different
  // spelling arrives as a string outside the union. Seeding the select with it
  // renders a blank `SelectValue` and fails only at `PUT`, with a 400. An
  // unrecognised value therefore seeds "inherit" and is reported explicitly
  // below instead of being silently shown as a blank field.
  const serverPolicy = asContractPolicy(kind.global?.contract_policy);
  const serverEnv = asEnvironment(kind.global?.environment);
  const unrecognised = [
    ["Contract policy", kind.global?.contract_policy, serverPolicy] as const,
    ["Environment", kind.global?.environment, serverEnv] as const
  ]
    .filter(([, stored, known]) => stored != null && known === null)
    .map(([label, stored]) => `${label} = "${stored}"`);

  const [draftPolicy, setDraftPolicy] = useState<AirwayContractPolicy | null>(serverPolicy);
  const [draftEnv, setDraftEnv] = useState<AirwayEnvironment | null>(serverEnv);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const dirty = draftPolicy !== serverPolicy || draftEnv !== serverEnv;

  // Lifted here (not inside PolicyPreview) so Save can gate its confirm step
  // on the exact same data the disclosure renders — no separate fetch, no
  // risk of the confirm reading a different policy than what's on screen.
  // BOTH admission axes are passed: `environment` is part of the query key and
  // of the request, so changing it is a cache miss rather than a silent reuse.
  const preview = usePolicyPreview(
    kind.source_kind,
    draftPolicy ?? undefined,
    draftEnv ?? undefined,
    { enabled: previewOpen }
  );
  const upsert = useUpsertAirwayGlobalConfig();
  const deleteGlobal = useDeleteAirwayGlobalConfig();

  // Never derive the gate from `preview.data` alone, never from
  // `preview.isSuccess` without `previewOpen`, and never without telling it
  // which draft is about to be saved — the gate verifies the body it trusts
  // was computed for that exact `(contract_policy, environment)`. See
  // `computeSaveGate`'s doc for the full list of what "unknown" covers.
  //
  // `"global"`: this card writes the kind's fleet-wide row, so a preview the
  // server fenced to this operator's platform scope answered only part of the
  // question. That is the one tier where a non-zero `out_of_scope_pipelines`
  // must confirm.
  const gate = computeSaveGate(
    preview,
    previewOpen,
    { contractPolicy: draftPolicy, environment: draftEnv },
    "global"
  );

  function invalidatePreview() {
    setPreviewOpen(false);
  }

  function doSave() {
    // Always both fields — a PUT is a replace, not a patch; omitting one
    // would silently wipe it back to "inherit".
    upsert.mutate(
      {
        sourceKind: kind.source_kind,
        body: { contract_policy: draftPolicy, environment: draftEnv }
      },
      { onSuccess: () => setConfirmOpen(false) }
    );
  }

  function handleSaveClick() {
    if (gate.kind === "clean") {
      doSave();
    } else {
      setConfirmOpen(true);
    }
  }

  // "Clear" removes the global row entirely (distinct from saving both
  // fields as `null`, which still leaves a row behind) — the un-set
  // direction, so it skips the confirm gate the same way an un-dirtying
  // edit would. Resets the drafts too, or `dirty` would immediately read
  // `true` again against the now-`null` server row.
  function handleClearClick() {
    deleteGlobal.mutate(kind.source_kind, {
      onSuccess: () => {
        setDraftPolicy(null);
        setDraftEnv(null);
        invalidatePreview();
      }
    });
  }

  return (
    <Card data-testid={`admin-airway-card-${kind.source_kind}`}>
      <CardHeader className='flex flex-row items-start justify-between gap-4 space-y-0'>
        <div>
          <h3 className='font-semibold text-sm'>{kind.source_kind}</h3>
          <p className='mt-0.5 text-muted-foreground text-xs'>
            {formatUpdatedAt(kind.global?.updated_at)}
          </p>
        </div>
        <div className='flex shrink-0 items-center gap-1.5'>
          {kind.global !== null && (
            <Button
              type='button'
              variant='outline'
              size='sm'
              disabled={upsert.isPending || deleteGlobal.isPending}
              onClick={handleClearClick}
              data-testid={`admin-airway-clear-${kind.source_kind}`}
              tooltip="Remove this kind's global row — every workspace falls back to airway's built-in default"
            >
              {deleteGlobal.isPending ? (
                <Spinner className='size-3.5' />
              ) : (
                <RotateCcw className='size-3.5' />
              )}
              Clear
            </Button>
          )}
          <Button
            type='button'
            size='sm'
            disabled={!dirty || upsert.isPending}
            onClick={handleSaveClick}
            data-testid={`admin-airway-save-${kind.source_kind}`}
          >
            {upsert.isPending && <Spinner className='size-3.5' />}
            Save
          </Button>
        </div>
      </CardHeader>

      <CardContent className='space-y-4'>
        {unrecognised.length > 0 && (
          <p
            className='rounded-md border border-status-warning-text/40 bg-status-warning-bg px-3 py-2 text-status-warning-text text-xs'
            data-testid={`admin-airway-unrecognised-${kind.source_kind}`}
          >
            This kind's stored row holds a value this build does not recognise (
            {unrecognised.join(", ")}). Airway refuses it at run time rather than falling back, so
            these pipelines are already failing. The fields below show "inherit"; saving replaces
            the stored value.
          </p>
        )}

        <PolicyFields
          idSuffix={kind.source_kind}
          draftPolicy={draftPolicy}
          draftEnv={draftEnv}
          onPolicyChange={(value) => {
            setDraftPolicy(value);
            invalidatePreview();
          }}
          onEnvChange={(value) => {
            setDraftEnv(value);
            invalidatePreview();
          }}
        />

        <PolicyPreview
          sourceKind={kind.source_kind}
          open={previewOpen}
          onRequestPreview={() => setPreviewOpen(true)}
          onHide={() => setPreviewOpen(false)}
          preview={preview}
        />

        <WorkspaceOverrides
          sourceKind={kind.source_kind}
          overrides={kind.overrides}
          global={kind.global}
        />
      </CardContent>

      <SaveConfirmDialog
        testIdSuffix={kind.source_kind}
        open={confirmOpen}
        pending={upsert.isPending}
        gate={gate}
        onOpenChange={setConfirmOpen}
        onConfirm={doSave}
        onPreviewInstead={() => setPreviewOpen(true)}
      />
    </Card>
  );
}
