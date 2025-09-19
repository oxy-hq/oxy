import React, { useState, useEffect, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AlertTriangle, CheckCircle, Loader2 } from "lucide-react";
import { useProjectStatus } from "@/hooks/useProjectStatus";
import { useBulkCreateSecrets } from "@/hooks/api/secrets/useSecretMutations";
import { toast } from "sonner";
import { useNavigate, useParams } from "react-router-dom";
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
    [configStatus?.required_secrets],
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
      [secretName]: value,
    }));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    // Validate all secrets are filled
    const emptySecrets = requiredSecrets.filter(
      (name) => !secretValues[name]?.trim(),
    );
    if (emptySecrets.length > 0) {
      toast.error(
        `Please fill in all required secrets: ${emptySecrets.join(", ")}`,
      );
      return;
    }

    setIsSubmitting(true);

    try {
      // Create all secrets in bulk
      const secretsToCreate = requiredSecrets.map((secretName) => ({
        name: secretName,
        value: secretValues[secretName],
      }));

      const result = await bulkCreateSecretsMutation.mutateAsync({
        secrets: secretsToCreate,
      });

      // Check if any secrets failed to create
      if (result.failed_secrets.length > 0) {
        const failedNames = result.failed_secrets
          .map((f) => f.secret.name)
          .join(", ");
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
    <div className="min-h-screen w-full overflow-y-auto bg-neutral-950 flex flex-col items-center">
      <div className="w-full flex-1 max-w-md flex flex-col gap-4 m-4">
        <div className="text-center">
          <AlertTriangle className="mx-auto h-12 w-12 text-orange-500 mb-4" />
          <h1 className="text-2xl font-bold text-foreground">
            Required Secrets Setup
          </h1>
          <p className="text-muted-foreground mt-2">
            Your configuration requires the following secrets to be configured
            before you can continue.
          </p>
        </div>

        <div className="bg-orange-50 dark:bg-orange-950 border border-orange-200 dark:border-orange-800 rounded-lg p-4 flex items-start gap-3">
          <AlertTriangle className="h-4 w-4 text-orange-500 mt-0.5 flex-shrink-0" />
          <p className="text-sm text-orange-800 dark:text-orange-200">
            These secrets are required for your project to function properly.
            Please provide values for all required secrets.
          </p>
        </div>

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          {requiredSecrets.map((secretName) => (
            <div key={secretName}>
              <Label htmlFor={secretName} className="text-sm font-medium">
                {secretName}
                <span className="text-destructive ml-1">*</span>
              </Label>
              <Input
                id={secretName}
                type="password"
                value={secretValues[secretName] || ""}
                onChange={(e) => handleInputChange(secretName, e.target.value)}
                placeholder={`Enter value for ${secretName}`}
                required
                disabled={isSubmitting}
              />
            </div>
          ))}

          <Button type="submit" className="w-full" disabled={isSubmitting}>
            {isSubmitting ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                Configuring Secrets...
              </>
            ) : (
              <>
                <CheckCircle className="w-4 h-4 mr-2" />
                Configure Secrets
              </>
            )}
          </Button>

          <Button
            type="button"
            variant="outline"
            onClick={() =>
              navigate(ROUTES.PROJECT(projectId!).ROOT, { replace: true })
            }
          >
            Skip for now
          </Button>
        </form>
      </div>
    </div>
  );
};

export default RequiredSecretsSetup;
