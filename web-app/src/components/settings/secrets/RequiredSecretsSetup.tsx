import { AlertTriangle, CheckCircle, Loader2 } from "lucide-react";
import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useBulkCreateSecrets } from "@/hooks/api/secrets/useSecretMutations";
import { useProjectStatus } from "@/hooks/useProjectStatus";
import ROUTES from "@/libs/utils/routes";

const RequiredSecretsSetup: React.FC = () => {
  const { projectId } = useParams<{ projectId: string }>();
  const navigate = useNavigate();
  const { data: configStatus, refetch } = useProjectStatus(projectId!);
  const bulkCreateSecretsMutation = useBulkCreateSecrets(projectId!);

  const [secretValues, setSecretValues] = useState<Record<string, string>>({});
  const [isSubmitting, setIsSubmitting] = useState(false);

  const requiredSecrets = useMemo(
    () => configStatus?.required_secrets || [],
    [configStatus?.required_secrets]
  );

  // Initialize secret values state
  useEffect(() => {
    const initialValues: Record<string, string> = {};
    requiredSecrets.forEach((secretName) => {
      initialValues[secretName] = "";
    });
    setSecretValues(initialValues);
  }, [requiredSecrets]);

  const handleInputChange = (secretName: string, value: string) => {
    setSecretValues((prev) => ({
      ...prev,
      [secretName]: value
    }));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    // Validate all secrets are filled
    const emptySecrets = requiredSecrets.filter((name) => !secretValues[name]?.trim());
    if (emptySecrets.length > 0) {
      toast.error(`Please fill in all required secrets: ${emptySecrets.join(", ")}`);
      return;
    }

    setIsSubmitting(true);

    try {
      // Create all secrets in bulk
      const secretsToCreate = requiredSecrets.map((secretName) => ({
        name: secretName,
        value: secretValues[secretName]
      }));

      const result = await bulkCreateSecretsMutation.mutateAsync({
        secrets: secretsToCreate
      });

      // Check if any secrets failed to create
      if (result.failed_secrets.length > 0) {
        const failedNames = result.failed_secrets.map((f) => f.secret.name).join(", ");
        toast.error(`Failed to create some secrets: ${failedNames}`);
        return;
      }

      toast.success("All required secrets have been configured successfully!");

      // Refetch config status to check if we can proceed
      await refetch();

      // Navigate back to home
      navigate(ROUTES.PROJECT(projectId!).ROOT, { replace: true });
    } catch (error) {
      console.error("Error creating secrets:", error);
      toast.error("Failed to create some secrets. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className='flex min-h-screen w-full flex-col items-center overflow-y-auto bg-neutral-950'>
      <div className='m-4 flex w-full max-w-md flex-1 flex-col gap-4'>
        <div className='text-center'>
          <AlertTriangle className='mx-auto mb-4 h-12 w-12 text-orange-500' />
          <h1 className='font-bold text-2xl text-foreground'>Required Secrets Setup</h1>
          <p className='mt-2 text-muted-foreground'>
            Your configuration requires the following secrets to be configured before you can
            continue.
          </p>
        </div>

        <div className='flex items-start gap-3 rounded-lg border border-orange-200 bg-orange-50 p-4 dark:border-orange-800 dark:bg-orange-950'>
          <AlertTriangle className='mt-0.5 h-4 w-4 flex-shrink-0 text-orange-500' />
          <p className='text-orange-800 text-sm dark:text-orange-200'>
            These secrets are required for your project to function properly. Please provide values
            for all required secrets.
          </p>
        </div>

        <form onSubmit={handleSubmit} className='flex flex-col gap-4'>
          {requiredSecrets.map((secretName) => (
            <div key={secretName}>
              <Label htmlFor={secretName} className='font-medium text-sm'>
                {secretName}
                <span className='ml-1 text-destructive'>*</span>
              </Label>
              <Input
                id={secretName}
                type='password'
                value={secretValues[secretName] || ""}
                onChange={(e) => handleInputChange(secretName, e.target.value)}
                placeholder={`Enter value for ${secretName}`}
                required
                disabled={isSubmitting}
              />
            </div>
          ))}

          <Button type='submit' className='w-full' disabled={isSubmitting}>
            {isSubmitting ? (
              <>
                <Loader2 className='mr-2 h-4 w-4 animate-spin' />
                Configuring Secrets...
              </>
            ) : (
              <>
                <CheckCircle className='mr-2 h-4 w-4' />
                Configure Secrets
              </>
            )}
          </Button>

          <Button
            type='button'
            variant='outline'
            onClick={() => navigate(ROUTES.PROJECT(projectId!).ROOT, { replace: true })}
          >
            Skip for now
          </Button>
        </form>
      </div>
    </div>
  );
};

export default RequiredSecretsSetup;
