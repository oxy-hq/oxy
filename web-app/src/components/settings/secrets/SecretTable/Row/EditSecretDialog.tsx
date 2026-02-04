import type React from "react";
import { useEffect, useState } from "react";
import { SecretInput } from "@/components/ui/SecretInput";
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
import { Textarea } from "@/components/ui/shadcn/textarea";
import { useUpdateSecret } from "@/hooks/api/secrets/useSecretMutations";
import type { Secret, SecretEditFormData, UpdateSecretRequest } from "@/types/secret";

interface EditSecretDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  secret: Secret | null;
  onSecretUpdated: () => void;
}

export const EditSecretDialog: React.FC<EditSecretDialogProps> = ({
  open,
  onOpenChange,
  secret,
  onSecretUpdated
}) => {
  const updateSecretMutation = useUpdateSecret();
  const [formData, setFormData] = useState<SecretEditFormData>({
    value: "",
    description: ""
  });
  const [errors, setErrors] = useState<{ [key: string]: string }>({});

  // Reset form when secret changes or dialog opens
  useEffect(() => {
    if (open && secret) {
      setFormData({
        value: "",
        description: secret.description || ""
      });
      setErrors({});
    }
  }, [open, secret]);

  const validateForm = (): boolean => {
    const newErrors: { [key: string]: string } = {};

    if (!formData.value?.trim()) {
      newErrors.value = "Secret value is required";
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleUpdateSecret = async () => {
    if (!secret || !validateForm()) {
      return;
    }

    try {
      const request: UpdateSecretRequest = {
        value: formData.value,
        description: formData.description?.trim() || undefined
      };

      await updateSecretMutation.mutateAsync({
        id: secret.id,
        request
      });

      onSecretUpdated();
    } catch (error) {
      console.error("Failed to update secret:", error);
      // Error toast is handled in the mutation hook
    }
  };

  const handleCancel = () => {
    setFormData({
      value: "",
      description: secret?.description || ""
    });
    setErrors({});
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-[425px]'>
        <DialogHeader>
          <DialogTitle>Edit Secret</DialogTitle>
          <DialogDescription>
            Update the secret value and description for "{secret?.name}". The value will be
            encrypted and cannot be viewed after saving.
          </DialogDescription>
        </DialogHeader>

        <div className='grid gap-4 py-4'>
          <div className='grid gap-2'>
            <Label htmlFor='edit-name'>Name</Label>
            <Input id='edit-name' value={secret?.name || ""} disabled className='bg-muted' />
            <p className='text-muted-foreground text-xs'>Secret name cannot be changed</p>
          </div>

          <div className='grid gap-2'>
            <Label htmlFor='edit-value'>New Value *</Label>
            <SecretInput
              id='edit-value'
              placeholder='Enter new secret value'
              value={formData.value}
              onChange={(e) => setFormData({ ...formData, value: e.target.value })}
              className={errors.value ? "border-destructive" : ""}
            />
            {errors.value && <p className='text-destructive text-sm'>{errors.value}</p>}
          </div>

          <div className='grid gap-2'>
            <Label htmlFor='edit-description'>Description</Label>
            <Textarea
              id='edit-description'
              placeholder='Optional description of this secret'
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
              rows={3}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant='outline' onClick={handleCancel}>
            Cancel
          </Button>
          <Button onClick={handleUpdateSecret} disabled={updateSecretMutation.isPending}>
            {updateSecretMutation.isPending ? "Updating..." : "Update Secret"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
