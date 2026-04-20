import { isAxiosError } from "axios";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useCreateOrg } from "@/hooks/api/organizations";
import type { Organization } from "@/types/organization";
import { slugify } from "@/utils/slugify";

export default function CreateOrgDialog({
  open,
  onOpenChange,
  onCreated
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: (org: Organization) => void;
}) {
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [slugTouched, setSlugTouched] = useState(false);
  const [slugError, setSlugError] = useState("");
  const createOrg = useCreateOrg();

  useEffect(() => {
    if (open) {
      setName("");
      setSlug("");
      setSlugTouched(false);
      setSlugError("");
    }
  }, [open]);

  const handleNameChange = (value: string) => {
    setName(value);
    if (!slugTouched) {
      setSlug(slugify(value));
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !slug.trim()) return;
    setSlugError("");

    try {
      const org = await createOrg.mutateAsync({ name: name.trim(), slug: slug.trim() });
      onCreated(org);
    } catch (err) {
      if (isAxiosError(err) && err.response?.status === 409) {
        setSlugError("This slug is already taken. Please choose a different one.");
      } else {
        toast.error("Failed to create organization");
      }
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>Create organization</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className='flex flex-col gap-4'>
          <div className='space-y-1.5'>
            <Label htmlFor='org-name'>Organization name</Label>
            <Input
              id='org-name'
              value={name}
              onChange={(e) => handleNameChange(e.target.value)}
              placeholder='Acme Inc'
              autoFocus
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='org-slug'>URL slug</Label>
            <Input
              id='org-slug'
              value={slug}
              onChange={(e) => {
                setSlugTouched(true);
                setSlug(e.target.value);
                setSlugError("");
              }}
              placeholder='acme-inc'
            />
            {slugError && <p className='text-destructive text-sm'>{slugError}</p>}
          </div>
          <Button
            type='submit'
            size='sm'
            disabled={!name.trim() || !slug.trim() || createOrg.isPending}
          >
            {createOrg.isPending ? "Creating…" : "Create organization"}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
