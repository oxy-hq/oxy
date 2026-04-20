import { BookOpen, Loader2, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { OnboardingService } from "@/services/api/onboarding";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { WorkspaceCreationType } from "../../types";
import { extractErrorMessage } from "../../utils";
import { GitHubOnboardingStep } from "./components/GitHubOnboardingStep";
import { OptionCard } from "./components/OptionCard";

type CreateStep = "pick" | "new" | "github" | "loading";

interface Props {
  open: boolean;
  onClose: () => void;
  onCreated: (workspaceId: string, type: WorkspaceCreationType) => void;
}

export function CreateWorkspaceDialog({ open, onClose, onCreated }: Props) {
  const { org } = useCurrentOrg();
  const [step, setStep] = useState<CreateStep>("pick");
  const [error, setError] = useState<string | null>(null);
  const [workspaceName, setWorkspaceName] = useState("");

  // Reset state every time the dialog opens so stale "loading" step doesn't persist.
  useEffect(() => {
    if (open) {
      setStep("pick");
      setError(null);
      setWorkspaceName("");
    }
  }, [open]);

  const handleClose = () => {
    setStep("pick");
    setError(null);
    setWorkspaceName("");
    onClose();
  };

  const handleDemo = async () => {
    setStep("loading");
    setError(null);
    try {
      if (!org?.id) {
        setError("Select an organization first.");
        setStep("pick");
        return;
      }
      const result = await OnboardingService.setupDemo(org.id);
      onCreated(result.workspace_id, "demo");
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to set up demo workspace");
      setStep("pick");
    }
  };

  const handleNewCreate = async () => {
    setStep("loading");
    setError(null);
    try {
      if (!org?.id) {
        setError("Select an organization first.");
        setStep("new");
        return;
      }
      const result = await OnboardingService.setupNew(org.id, workspaceName.trim() || undefined);
      onCreated(result.workspace_id, "new");
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to create workspace");
      setStep("new");
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>
            {step === "github"
              ? "Import from GitHub"
              : step === "new"
                ? "Create blank workspace"
                : "New workspace"}
          </DialogTitle>
        </DialogHeader>

        {step === "loading" && (
          <div className='flex flex-col items-center gap-3 py-10'>
            <Loader2 className='h-5 w-5 animate-spin text-primary' />
            <p className='text-muted-foreground text-sm'>Setting up workspace…</p>
          </div>
        )}

        {step === "pick" && (
          <div className='flex flex-col gap-5'>
            <div className='flex flex-col gap-2'>
              <OptionCard
                label='01'
                icon={<GithubIcon className='h-3.5 w-3.5' />}
                title='Import from GitHub'
                description='Clone an existing repository and start working immediately.'
                badge='Recommended'
                onClick={() => setStep("github")}
              />
              <OptionCard
                label='02'
                icon={<BookOpen className='h-3.5 w-3.5' />}
                title='Demo Workspace'
                description='Explore Oxy with pre-built sample data and example queries.'
                onClick={handleDemo}
              />
              <OptionCard
                label='03'
                icon={<Plus className='h-3.5 w-3.5' />}
                title='Blank Workspace'
                description='Start from scratch with an empty workspace.'
                onClick={() => {
                  setWorkspaceName("");
                  setError(null);
                  setStep("new");
                }}
              />
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}
          </div>
        )}

        {step === "new" && (
          <div className='flex flex-col gap-4'>
            <div className='space-y-1.5'>
              <Label htmlFor='new-ws-name'>Workspace name</Label>
              <Input
                id='new-ws-name'
                placeholder='my-workspace'
                value={workspaceName}
                onChange={(e) => setWorkspaceName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleNewCreate();
                }}
                autoFocus
              />
              <p className='text-muted-foreground text-xs'>Leave blank for a default name.</p>
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}

            <div className='flex gap-2'>
              <Button
                variant='outline'
                size='sm'
                className='flex-1 text-xs'
                onClick={() => {
                  setError(null);
                  setStep("pick");
                }}
              >
                Back
              </Button>
              <Button size='sm' className='flex-1 text-xs' onClick={handleNewCreate}>
                Create workspace
              </Button>
            </div>
          </div>
        )}

        {step === "github" && (
          <GitHubOnboardingStep
            onBack={() => setStep("pick")}
            onDone={(workspaceId) => onCreated(workspaceId, "github")}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}
