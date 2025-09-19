import React, { useState } from "react";
import { useCreateOrganization } from "@/hooks/api/organizations/useOrganizations";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Plus, Loader2 } from "lucide-react";
import { toast } from "sonner";

interface NewOrganizationDialogProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}

const NewOrganizationDialog = ({
  isOpen,
  onOpenChange,
}: NewOrganizationDialogProps) => {
  const [newOrgName, setNewOrgName] = useState("");
  const [isCreating, setIsCreating] = useState(false);

  const createOrgMutation = useCreateOrganization();

  const handleCreateOrganization = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newOrgName.trim()) return;

    setIsCreating(true);
    try {
      await createOrgMutation.mutateAsync({ name: newOrgName.trim() });
      setNewOrgName("");
      onOpenChange(false);
      toast.success("Organization created successfully!");
    } catch (error) {
      toast.error("Failed to create organization");
      console.error("Error creating organization:", error);
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button size={"sm"}>
          <Plus className="h-4 w-4 mr-2" />
          New organization
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create New Organization</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleCreateOrganization} className="space-y-4">
          <Input
            placeholder="Organization name"
            value={newOrgName}
            onChange={(e) => setNewOrgName(e.target.value)}
            disabled={isCreating}
            autoFocus
          />
          <div className="flex gap-3 justify-end">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isCreating}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isCreating || !newOrgName.trim()}>
              {isCreating ? (
                <Loader2 className="h-4 w-4 animate-spin mr-2" />
              ) : (
                <Plus className="h-4 w-4 mr-2" />
              )}
              Create
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default NewOrganizationDialog;
