import { Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Textarea } from "@/components/ui/shadcn/textarea";
import {
  useDeleteAppIntegration,
  useUpsertAppIntegration
} from "@/hooks/api/appIntegrations/useAppIntegrationMutations";
import type {
  AppIntegration,
  AppIntegrationKind,
  UpsertAppIntegrationBody
} from "@/services/api/appIntegrations";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { ToastWebhookSetup } from "./ToastWebhookSetup";

interface ConfigureAppDialogProps {
  kind: AppIntegrationKind;
  existing: AppIntegration | null;
  onClose: () => void;
}

const TITLE: Record<AppIntegrationKind, string> = {
  toast: "Configure Toast POS",
  openweathermap: "Configure OpenWeatherMap",
  besttime: "Configure BestTime",
  unifi: "Configure UniFi Cameras"
};

const DEFAULT_NAME: Record<AppIntegrationKind, string> = {
  toast: "toast",
  openweathermap: "openweathermap",
  besttime: "besttime",
  unifi: "unifi"
};

const DEFAULT_VAR: Record<AppIntegrationKind, string> = {
  toast: "TOAST_WEBHOOK_SECRET",
  openweathermap: "OPENWEATHERMAP_API_KEY",
  besttime: "BESTTIME_API_KEY",
  unifi: "UNIFI_API_KEY"
};

export function ConfigureAppDialog({ kind, existing, onClose }: ConfigureAppDialogProps) {
  const upsert = useUpsertAppIntegration();
  const remove = useDeleteAppIntegration();
  const workspace = useCurrentWorkspace((s) => s.workspace);

  const [name, setName] = useState(existing?.name ?? DEFAULT_NAME[kind]);
  const [varName, setVarName] = useState<string>(initialVarName(kind, existing));
  const [guidText, setGuidText] = useState<string>(
    existing?.kind === "toast" ? existing.restaurant_guids.join("\n") : ""
  );
  const [errors, setErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    setName(existing?.name ?? DEFAULT_NAME[kind]);
    setVarName(initialVarName(kind, existing));
    setGuidText(existing?.kind === "toast" ? existing.restaurant_guids.join("\n") : "");
    setErrors({});
  }, [kind, existing]);

  const handleSubmit = async () => {
    const trimmedName = name.trim();
    const trimmedVar = varName.trim();
    const next: Record<string, string> = {};
    if (!trimmedName) next.name = "Name is required";
    if (!trimmedVar) next.var = "Secret variable name is required";
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmedVar))
      next.var = "Use uppercase letters, digits, and underscores (e.g. TOAST_WEBHOOK_SECRET)";
    setErrors(next);
    if (Object.keys(next).length > 0) return;

    const body = buildBody(kind, trimmedName, trimmedVar, parseGuids(guidText));
    await upsert.mutateAsync(body);
    onClose();
  };

  const handleDelete = async () => {
    await remove.mutateAsync(kind);
    onClose();
  };

  const varLabel = kind === "toast" ? "Webhook secret variable" : "API key variable";

  return (
    <Dialog open onOpenChange={(v) => !v && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>{TITLE[kind]}</DialogTitle>
          <DialogDescription>
            Set the secret variable name. The actual value is stored in the workspace Secrets tab.
          </DialogDescription>
        </DialogHeader>

        <div className='flex flex-col gap-4 py-2'>
          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='app-name'>Entry name</Label>
            <Input
              id='app-name'
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={DEFAULT_NAME[kind]}
            />
            {errors.name && <span className='text-destructive text-xs'>{errors.name}</span>}
          </div>

          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='app-var'>{varLabel}</Label>
            <Input
              id='app-var'
              value={varName}
              onChange={(e) => setVarName(e.target.value)}
              placeholder={DEFAULT_VAR[kind]}
              className='font-mono'
            />
            {errors.var && <span className='text-destructive text-xs'>{errors.var}</span>}
          </div>

          {kind === "toast" && (
            <div className='flex flex-col gap-1.5'>
              <Label htmlFor='app-guids'>Restaurant GUIDs (one per line, optional)</Label>
              <Textarea
                id='app-guids'
                value={guidText}
                onChange={(e) => setGuidText(e.target.value)}
                placeholder='Leave empty to accept any restaurant.'
                rows={4}
                className='font-mono text-xs'
              />
            </div>
          )}

          {kind === "toast" && <ToastWebhookSetup projectId={workspace?.id ?? ""} />}
        </div>

        <DialogFooter className='flex items-center justify-between gap-2 sm:flex-row-reverse'>
          <div className='flex gap-2'>
            <Button variant='outline' onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={handleSubmit} disabled={upsert.isPending || remove.isPending}>
              {upsert.isPending ? <Spinner className='size-4' /> : "Save"}
            </Button>
          </div>
          {existing && (
            <Button
              variant='ghost'
              onClick={handleDelete}
              disabled={upsert.isPending || remove.isPending}
              className='text-destructive hover:text-destructive'
            >
              <Trash2 className='size-4' />
              Disconnect
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function initialVarName(kind: AppIntegrationKind, existing: AppIntegration | null): string {
  if (!existing) return DEFAULT_VAR[kind];
  if (existing.kind === "toast") return existing.webhook_secret_var;
  return existing.api_key_var;
}

function parseGuids(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function buildBody(
  kind: AppIntegrationKind,
  name: string,
  varName: string,
  guids: string[]
): UpsertAppIntegrationBody {
  switch (kind) {
    case "toast":
      return { kind, name, webhook_secret_var: varName, restaurant_guids: guids };
    case "openweathermap":
      return { kind, name, api_key_var: varName };
    case "besttime":
      return { kind, name, api_key_var: varName };
    case "unifi":
      return { kind, name, api_key_var: varName };
  }
}
