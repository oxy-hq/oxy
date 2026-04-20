import { isAxiosError } from "axios";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useDeleteOrg, useUpdateOrg } from "@/hooks/api/organizations";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { Organization } from "@/types/organization";

interface GeneralTabProps {
  org: Organization;
}

export default function GeneralTab({ org }: GeneralTabProps) {
  const navigate = useNavigate();
  const [name, setName] = useState(org.name);
  const [slug, setSlug] = useState(org.slug);
  const updateOrg = useUpdateOrg();
  const deleteOrg = useDeleteOrg();
  const { setOrg, clearOrg } = useCurrentOrg();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [slugError, setSlugError] = useState("");

  const handleSave = async () => {
    setSlugError("");
    try {
      const updated = await updateOrg.mutateAsync({
        orgId: org.id,
        data: { name: name.trim(), slug: slug.trim() }
      });
      setOrg(updated);
      toast.success("Organization updated");
    } catch (err) {
      if (isAxiosError(err) && err.response?.status === 409) {
        setSlugError("This slug is already taken. Please choose a different one.");
      } else {
        toast.error("Failed to update organization");
      }
    }
  };

  const handleDelete = async () => {
    try {
      await deleteOrg.mutateAsync(org.id);
      clearOrg();
      toast.success("Organization deleted");
      navigate(ROUTES.ROOT);
    } catch {
      toast.error("Failed to delete organization");
    }
  };

  const isOwner = org.role === "owner";
  const hasChanges = name.trim() !== org.name || slug.trim() !== org.slug;

  return (
    <div className='space-y-8'>
      <div className='space-y-4'>
        <div className='space-y-2'>
          <Label htmlFor='org-name'>Organization name</Label>
          <Input id='org-name' value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className='space-y-2'>
          <Label htmlFor='org-slug'>URL slug</Label>
          <Input
            id='org-slug'
            value={slug}
            onChange={(e) => {
              setSlug(e.target.value);
              setSlugError("");
            }}
          />
          {slugError && <p className='text-destructive text-sm'>{slugError}</p>}
        </div>
        <Button onClick={handleSave} disabled={!hasChanges || updateOrg.isPending}>
          {updateOrg.isPending ? "Saving..." : "Save"}
        </Button>
      </div>

      {isOwner && (
        <div className='space-y-4 rounded-lg border border-destructive/50 p-4'>
          <h3 className='font-medium text-destructive'>Danger Zone</h3>
          <p className='text-muted-foreground text-sm'>
            Deleting this organization will permanently remove all workspaces, members, and data.
          </p>
          {!confirmDelete ? (
            <Button variant='destructive' onClick={() => setConfirmDelete(true)}>
              Delete Organization
            </Button>
          ) : (
            <div className='flex items-center gap-2'>
              <Button variant='destructive' onClick={handleDelete} disabled={deleteOrg.isPending}>
                {deleteOrg.isPending ? "Deleting..." : "Confirm Delete"}
              </Button>
              <Button variant='outline' onClick={() => setConfirmDelete(false)}>
                Cancel
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
