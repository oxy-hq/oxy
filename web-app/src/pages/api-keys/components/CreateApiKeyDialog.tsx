import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { DatePicker } from "@/components/ui/shadcn/date-picker";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { toast } from "sonner";
import {
  ApiKeyFormData,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
} from "@/types/apiKey";
import { useCreateApiKey } from "@/hooks/api/apiKeys/useApiKeyMutations";

interface CreateApiKeyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onApiKeyCreated: (apiKey: CreateApiKeyResponse) => void;
}

export const CreateApiKeyDialog: React.FC<CreateApiKeyDialogProps> = ({
  open,
  onOpenChange,
  onApiKeyCreated,
}) => {
  const createApiKeyMutation = useCreateApiKey();
  const [formData, setFormData] = useState<ApiKeyFormData>({
    name: "",
    expiresAt: undefined,
  });

  const handleCreateApiKey = async () => {
    if (!formData.name.trim()) {
      toast.error("Please enter a name for the API key");
      return;
    }

    const request: CreateApiKeyRequest = {
      name: formData.name.trim(),
      expires_at: formData.expiresAt?.toISOString(),
    };

    const response = await createApiKeyMutation.mutateAsync(request);
    onApiKeyCreated(response);

    // Reset form and close dialog
    setFormData({ name: "", expiresAt: undefined });
    onOpenChange(false);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md bg-neutral-900">
        <DialogHeader>
          <DialogTitle>Create API Key</DialogTitle>
          <DialogDescription>
            Create a new API key for programmatic access to your account.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="flex flex-col gap-2 ">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              placeholder="e.g., Production API Key"
              value={formData.name}
              onChange={(e) =>
                setFormData({ ...formData, name: e.target.value })
              }
            />
          </div>

          <div className="flex flex-col gap-2 ">
            <Label htmlFor="expires">Expiration Date (Optional)</Label>
            <DatePicker
              date={formData.expiresAt}
              onSelect={(date) => setFormData({ ...formData, expiresAt: date })}
              placeholder="Select expiration date"
              minDate={new Date()}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleCreateApiKey}>Create API Key</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
