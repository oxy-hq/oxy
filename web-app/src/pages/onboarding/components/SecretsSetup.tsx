import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/shadcn/card";
import { SecretInput } from "@/components/ui/SecretInput";
import { AlertTriangle, Loader2 } from "lucide-react";
import { SecretInputFormData } from "@/types/config";
import { useCreateSecret } from "@/hooks/api/useSecretMutations";
import { toast } from "sonner";

interface SecretsSetupProps {
  missingSecrets: string[];
  onSecretsSetup: () => void;
  onSkip?: () => void;
}

export const SecretsSetup: React.FC<SecretsSetupProps> = ({
  missingSecrets,
  onSecretsSetup,
}) => {
  const [formData, setFormData] = useState<SecretInputFormData>({});
  const [isSubmitting, setIsSubmitting] = useState(false);
  const createSecretMutation = useCreateSecret();

  const handleInputChange = (secretName: string, value: string) => {
    setFormData((prev) => ({
      ...prev,
      [secretName]: value,
    }));
  };

  const handleSubmit = async () => {
    setIsSubmitting(true);

    try {
      // Create secrets for each missing secret that has a value
      const secretsToCreate = missingSecrets.filter((secret) =>
        formData[secret]?.trim(),
      );

      if (secretsToCreate.length === 0) {
        toast.error("Please provide at least one secret value");
        return;
      }

      // Create all secrets
      for (const secret of secretsToCreate) {
        const value = formData[secret].trim();
        await createSecretMutation.mutateAsync({
          name: secret,
          value,
          description: "",
        });
      }

      toast.success(
        `Successfully created ${secretsToCreate.length} secret${
          secretsToCreate.length > 1 ? "s" : ""
        }`,
      );

      onSecretsSetup();
    } catch (error) {
      console.error("Failed to create secrets:", error);
      toast.error("Failed to create secrets. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  const canSubmit = missingSecrets.some((secret) => formData[secret]?.trim());

  return (
    <Card className="w-full mx-auto">
      <CardHeader>
        <div className="flex items-center space-x-2">
          <AlertTriangle className="h-5 w-5 text-amber-500" />
          <h1>Required Secrets Setup</h1>
        </div>
        <CardDescription>
          Your project configuration requires some secrets to be configured.
          Please provide the missing values below to continue.
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-6">
        {missingSecrets.map((secret) => (
          <div key={secret} className="space-y-2">
            <Label htmlFor={secret} className="text-sm font-medium">
              {secret}
            </Label>
            <SecretInput
              id={secret}
              value={formData[secret] || ""}
              onChange={(e) => handleInputChange(secret, e.target.value)}
              placeholder={`Enter ${secret}`}
            />
          </div>
        ))}

        <Button
          onClick={handleSubmit}
          disabled={!canSubmit || isSubmitting}
          className="flex-1 sm:flex-none w-full"
        >
          {isSubmitting ? (
            <>
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
              Saving secrets...
            </>
          ) : (
            <>Proceed</>
          )}
        </Button>

        <div className="text-xs text-muted-foreground">
          <p>
            <strong>Note:</strong> Secrets will be stored securely in the secret
            manager. You can also provide these values as environment variables
            if preferred.
          </p>
        </div>
      </CardContent>
    </Card>
  );
};
