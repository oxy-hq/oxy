import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import type { UnifiImportResult, UnifiPreviewResult } from "@/services/api";
import AirhouseGate from "./components/AirhouseGate";
import ApiKeyStep from "./components/ApiKeyStep";
import DoneStep from "./components/DoneStep";
import PreviewStep from "./components/PreviewStep";

/**
 * Multi-step UniFi onboarding state. Each variant carries the data the
 * next step needs — no separate refs/state for "the api key" or "the
 * preview result", just the union.
 *
 * `gate` is the entrypoint. The Airhouse-before-UniFi check is
 * structural (see project_camera_fleet_schema_gating in memory): camera
 * events have nowhere to land without a provisioned Airhouse tenant.
 */
type Step =
  | { kind: "gate" }
  | { kind: "api-key" }
  | { kind: "preview"; apiKey: string; data: UnifiPreviewResult }
  | { kind: "done"; result: UnifiImportResult };

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Fires when the wizard finishes; parent can switch tabs etc. */
  onImported?: (result: UnifiImportResult) => void;
};

const UnifiSetupDialog: React.FC<Props> = ({ open, onOpenChange, onImported }) => {
  const [step, setStep] = useState<Step>({ kind: "gate" });

  // Reset internal state every time the dialog opens so re-entering
  // after a previous run doesn't show the old success page.
  const handleOpenChange = (next: boolean) => {
    if (next && !open) {
      setStep({ kind: "gate" });
    }
    onOpenChange(next);
  };

  const handleImported = (result: UnifiImportResult) => {
    toast.success(
      `Imported ${result.sites_upserted} site${result.sites_upserted === 1 ? "" : "s"} and ${result.cameras_upserted} camera${result.cameras_upserted === 1 ? "" : "s"}.`
    );
    setStep({ kind: "done", result });
    onImported?.(result);
  };

  const handleClose = () => {
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className='max-w-2xl'>
        <DialogHeader>
          <DialogTitle>Connect UniFi</DialogTitle>
          <DialogDescription>
            Import your UniFi sites and cameras into this workspace.
          </DialogDescription>
        </DialogHeader>

        {step.kind === "gate" && <AirhouseGate onReady={() => setStep({ kind: "api-key" })} />}

        {step.kind === "api-key" && (
          <ApiKeyStep onSuccess={(apiKey, data) => setStep({ kind: "preview", apiKey, data })} />
        )}

        {step.kind === "preview" && (
          <PreviewStep
            apiKey={step.apiKey}
            data={step.data}
            onBack={() => setStep({ kind: "api-key" })}
            onImported={handleImported}
          />
        )}

        {step.kind === "done" && <DoneStep result={step.result} onClose={handleClose} />}
      </DialogContent>
    </Dialog>
  );
};

export default UnifiSetupDialog;
