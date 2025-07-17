import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { SecretInput } from "@/components/ui/SecretInput";
import { validateSecretName } from "@/libs/utils";
import {
  SecretFormData,
  CreateSecretRequest,
  CreateSecretResponse,
} from "@/types/secret";
import { useCreateSecret } from "@/hooks/api/useSecretMutations";

interface CreateSecretDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSecretCreated: (secret: CreateSecretResponse) => void;
}

export const CreateSecretDialog: React.FC<CreateSecretDialogProps> = ({
  open,
  onOpenChange,
  onSecretCreated,
}) => {
  const createSecretMutation = useCreateSecret();
  const [formData, setFormData] = useState<SecretFormData>({
    name: "",
    value: "",
    description: "",
  });
  const [errors, setErrors] = useState<{ [key: string]: string }>({});

  const validateForm = (): boolean => {
    const newErrors: { [key: string]: string } = {};

    // Use the utility function for name validation
    const nameValidation = validateSecretName(formData.name);
    if (!nameValidation.isValid) {
      newErrors.name = nameValidation.error!;
    }

    if (!formData.value.trim()) {
      newErrors.value = "Secret value is required";
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleCreateSecret = async () => {
    if (!validateForm()) {
      return;
    }

    try {
      const request: CreateSecretRequest = {
        name: formData.name.trim(),
        value: formData.value,
        description: formData.description?.trim() || undefined,
      };

      const response = await createSecretMutation.mutateAsync(request);
      onSecretCreated(response);

      // Reset form
      setFormData({
        name: "",
        value: "",
        description: "",
      });
      setErrors({});
    } catch (error) {
      console.error("Failed to create secret:", error);
      // Error toast is handled in the mutation hook
    }
  };

  const handleCancel = () => {
    setFormData({
      name: "",
      value: "",
      description: "",
    });
    setErrors({});
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Create New Secret</DialogTitle>
          <DialogDescription>
            Store a new secret value securely. The value will be encrypted and
            cannot be viewed after creation.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-4">
          <div className="grid gap-2">
            <Label htmlFor="name">Name *</Label>
            <Input
              id="name"
              placeholder="e.g., DATABASE_PASSWORD, API_KEY"
              value={formData.name}
              onChange={(e) =>
                setFormData({ ...formData, name: e.target.value })
              }
              className={errors.name ? "border-destructive" : ""}
            />
            {errors.name && (
              <p className="text-sm text-destructive">{errors.name}</p>
            )}
          </div>

          <div className="grid gap-2">
            <Label htmlFor="value">Value *</Label>
            <SecretInput
              id="value"
              placeholder="Enter secret value"
              value={formData.value}
              onChange={(e) =>
                setFormData({ ...formData, value: e.target.value })
              }
              className={errors.value ? "border-destructive" : ""}
            />
            {errors.value && (
              <p className="text-sm text-destructive">{errors.value}</p>
            )}
          </div>

          <div className="grid gap-2">
            <Label htmlFor="description">Description</Label>
            <Textarea
              id="description"
              placeholder="Optional description of this secret"
              value={formData.description}
              onChange={(e) =>
                setFormData({ ...formData, description: e.target.value })
              }
              rows={3}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleCancel}>
            Cancel
          </Button>
          <Button
            onClick={handleCreateSecret}
            disabled={createSecretMutation.isPending}
          >
            {createSecretMutation.isPending ? "Creating..." : "Create Secret"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
