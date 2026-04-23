import { BookOpen, Loader2, Plus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import {
  useSetupDemoWorkspace,
  useSetupEmptyWorkspace
} from "@/hooks/api/workspaces/useLocalWorkspaceSetup";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { OptionCard } from "../CreateWorkspaceDialog/components/OptionCard";

type Step = "pick" | "loading" | "skipped";

const reload = () => window.location.reload();

export function LocalWorkspaceSetupDialog() {
  const [step, setStep] = useState<Step>("pick");
  const [skippedFiles, setSkippedFiles] = useState<string[]>([]);

  const setupEmpty = useSetupEmptyWorkspace(LOCAL_WORKSPACE_ID);
  const setupDemo = useSetupDemoWorkspace(LOCAL_WORKSPACE_ID);

  const handleEmpty = async () => {
    setStep("loading");
    try {
      await setupEmpty.mutateAsync();
      reload();
    } catch (err) {
      toast.error(extractMessage(err, "Failed to create empty workspace"));
      setStep("pick");
    }
  };

  const handleDemo = async () => {
    setStep("loading");
    try {
      const result = await setupDemo.mutateAsync();
      if (result.files_skipped.length > 0) {
        setSkippedFiles(result.files_skipped);
        setStep("skipped");
        return;
      }
      reload();
    } catch (err) {
      toast.error(extractMessage(err, "Failed to create demo workspace"));
      setStep("pick");
    }
  };

  // Non-dismissable: no close button, no onOpenChange that closes.
  return (
    <Dialog open onOpenChange={() => {}}>
      <DialogContent
        className='sm:max-w-md'
        onInteractOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>
            Welcome to Oxy — set up your workspace
          </DialogTitle>
        </DialogHeader>

        {step === "loading" && (
          <div className='flex flex-col items-center gap-3 py-10'>
            <Loader2 className='h-5 w-5 animate-spin text-primary' />
            <p className='text-muted-foreground text-sm'>Setting up workspace…</p>
          </div>
        )}

        {step === "pick" && (
          <div className='flex flex-col gap-2'>
            <p className='mb-2 text-muted-foreground text-sm'>
              No <code className='rounded bg-muted px-1'>config.yml</code> found in this directory.
              Pick how you'd like to start.
            </p>
            <OptionCard
              label='01'
              icon={<BookOpen className='h-3.5 w-3.5' />}
              title='Demo workspace'
              description='Explore Oxy with a sample project including example agents and SQL.'
              onClick={handleDemo}
            />
            <OptionCard
              label='02'
              icon={<Plus className='h-3.5 w-3.5' />}
              title='Empty workspace'
              description="Start from scratch. We'll create a minimal config.yml here."
              onClick={handleEmpty}
            />
          </div>
        )}

        {step === "skipped" && (
          <div className='flex flex-col gap-4'>
            <p className='text-sm'>
              Demo workspace created. {skippedFiles.length} existing file
              {skippedFiles.length === 1 ? " was" : "s were"} left untouched:
            </p>
            <ul className='max-h-40 overflow-y-auto rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-muted-foreground text-xs'>
              {skippedFiles.map((file) => (
                <li key={file}>{file}</li>
              ))}
            </ul>
            <Button size='sm' onClick={reload}>
              Continue
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function extractMessage(err: unknown, fallback: string): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null) {
    const withResponse = err as { response?: { data?: { error?: string; details?: string } } };
    const data = withResponse.response?.data;
    if (data?.error) return `${data.error}${data.details ? `: ${data.details}` : ""}`;
  }
  return fallback;
}
