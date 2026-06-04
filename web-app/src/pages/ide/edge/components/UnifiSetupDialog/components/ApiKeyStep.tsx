import { AlertCircle } from "lucide-react";
import type React from "react";
import { useForm } from "react-hook-form";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useUnifiPreview from "@/hooks/api/cameras/useUnifiPreview";
import type { UnifiPreviewResult } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

type FormValues = {
  apiKey: string;
};

type Props = {
  onSuccess: (apiKey: string, data: UnifiPreviewResult) => void;
};

/**
 * Plain RHF + inline `register("...", { required })` — matches the
 * shape used by AddDatabaseForm. We started with zod here but the
 * codebase isn't actually using zod anywhere (despite a stale note in
 * CLAUDE.md), and dragging in `@hookform/resolvers` + `zod` just for
 * one required-string check wasn't a net win.
 */
const ApiKeyStep: React.FC<Props> = ({ onSuccess }) => {
  const { workspace } = useCurrentWorkspace();
  const preview = useUnifiPreview(workspace?.id);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting }
  } = useForm<FormValues>({
    defaultValues: { apiKey: "" }
  });

  const onSubmit = async ({ apiKey }: FormValues) => {
    const data = await preview.mutateAsync({ apiKey: apiKey.trim() });
    onSuccess(apiKey.trim(), data);
  };

  return (
    <form className='flex flex-col gap-4' onSubmit={handleSubmit(onSubmit)}>
      <div className='flex flex-col gap-2'>
        <Label htmlFor='unifi-api-key'>UniFi API key</Label>
        <Input
          id='unifi-api-key'
          type='password'
          autoComplete='off'
          spellCheck={false}
          placeholder='Generated at api.ui.com'
          aria-invalid={!!errors.apiKey}
          {...register("apiKey", {
            required: "API key is required",
            validate: (v) => v.trim().length > 0 || "API key is required"
          })}
        />
        {errors.apiKey && <p className='text-destructive text-xs'>{errors.apiKey.message}</p>}
        <p className='text-muted-foreground text-xs'>
          We use this once to fetch the list of sites + cameras. It isn't stored.
        </p>
      </div>

      {preview.isError && (
        <Alert variant='destructive'>
          <AlertCircle />
          <AlertTitle>Couldn't reach UniFi</AlertTitle>
          <AlertDescription>{(preview.error as Error).message}</AlertDescription>
        </Alert>
      )}

      <div className='flex justify-end'>
        <Button type='submit' disabled={isSubmitting || preview.isPending}>
          {preview.isPending ? <Spinner /> : "Preview sites"}
        </Button>
      </div>
    </form>
  );
};

export default ApiKeyStep;
