import { ImageUp, Trash2 } from "lucide-react";
import { useRef } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  useAdminOrgLogo,
  useDeleteAdminOrgLogo,
  useUploadAdminOrgLogo
} from "@/hooks/api/adminTenants/useAdminOrgs";
import { OrgLogo } from "./OrgLogo";

const ACCEPT = ["image/png", "image/jpeg", "image/svg+xml", "image/webp", "image/gif"];
const MAX_BYTES = 1024 * 1024;

/**
 * Admin logo control for a tenant org — preview + upload / replace / remove.
 * Mirrors the org-settings logo control, but drives the admin-gated endpoints
 * (an Oxy staffer isn't a member of the tenant, so the org-admin flow can't be
 * reused). Same 1 MB / image-only constraints, validated client-side.
 */
export const OrgLogoEditor = ({ orgId, name }: { orgId: string; name: string }) => {
  const { data: dataUrl } = useAdminOrgLogo(orgId);
  const upload = useUploadAdminOrgLogo();
  const remove = useDeleteAdminOrgLogo();
  const fileRef = useRef<HTMLInputElement>(null);
  const busy = upload.isPending || remove.isPending;

  const handleFile = (file?: File) => {
    if (!file) return;
    if (!ACCEPT.includes(file.type)) {
      toast.error("Use a PNG, JPEG, SVG, WebP, or GIF image");
      return;
    }
    if (file.size > MAX_BYTES) {
      toast.error("Logo must be under 1 MB");
      return;
    }
    upload.mutate({ orgId, file });
  };

  return (
    <div className='flex items-center gap-4'>
      <OrgLogo orgId={orgId} name={name} size='lg' />
      <div className='flex flex-col gap-1.5'>
        <div className='flex items-center gap-2'>
          <input
            ref={fileRef}
            type='file'
            accept={ACCEPT.join(",")}
            className='hidden'
            onChange={(e) => {
              handleFile(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
          <Button
            variant='outline'
            size='sm'
            onClick={() => fileRef.current?.click()}
            disabled={busy}
          >
            <ImageUp className='size-3.5' />
            {dataUrl ? "Replace" : "Upload"}
          </Button>
          {dataUrl ? (
            <Button
              variant='ghost'
              size='sm'
              onClick={() => remove.mutate(orgId)}
              disabled={busy}
              className='text-destructive hover:bg-destructive/10 hover:text-destructive'
            >
              <Trash2 className='size-3.5' />
              Remove
            </Button>
          ) : null}
        </div>
        <p className='text-muted-foreground text-xs'>
          PNG, JPEG, SVG, WebP or GIF · up to 1 MB. White-labels the org's HQ chrome.
        </p>
      </div>
    </div>
  );
};
