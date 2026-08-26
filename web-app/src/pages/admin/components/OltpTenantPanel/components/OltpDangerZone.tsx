import { ChevronRight, Loader2, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Input } from "@/components/ui/shadcn/input";
import type { useDeprovisionOltp } from "@/hooks/api/oltp/useAdminOltp";
import { cn } from "@/libs/shadcn/utils";
import type { OltpConnectionInfo } from "@/services/api/oltp";

/**
 * Destroy the org's database.
 *
 * **Collapsed.** A permanently-open red-bordered box on a page an operator
 * opens to read a connection string put the most destructive control in the
 * eyeline every visit, and cost 100px doing it. Opening it is one click and is
 * itself a statement of intent.
 *
 * Typing the database name back, rather than an "are you sure": on a managed
 * provider this deletes a real billing resource and cannot be undone from here
 * — the same bar the CLI sets with `--yes`.
 *
 * Rendered for every tenant that has a row, not only `active` ones. A `failed`
 * or `pending_delete` tenant is exactly the state this exists to clear up.
 */
export const OltpDangerZone = ({
  data,
  deprovision
}: {
  data: OltpConnectionInfo;
  deprovision: ReturnType<typeof useDeprovisionOltp>;
}) => {
  const [open, setOpen] = useState(false);
  const [confirmDrop, setConfirmDrop] = useState("");

  return (
    <Collapsible open={open} onOpenChange={setOpen} data-testid='admin-org-oltp-danger-zone'>
      <CollapsibleTrigger className='flex items-center gap-1 text-muted-foreground text-xs transition-colors hover:text-destructive'>
        <ChevronRight className={cn("size-3 transition-transform", open && "rotate-90")} />
        Deprovision this database
      </CollapsibleTrigger>
      <CollapsibleContent className='pt-2'>
        <div className='flex flex-col gap-2 rounded border border-destructive/40 p-2'>
          <p className='text-muted-foreground text-xs'>
            Destroys <span className='font-mono text-foreground'>{data.database}</span> and
            everything in it.
            {data.provider !== "local" &&
              " The provider project is deleted and cannot be recovered from here."}
          </p>
          <div className='flex items-center gap-1.5'>
            <Input
              className='h-7 font-mono text-xs'
              placeholder={`Type ${data.database} to confirm`}
              value={confirmDrop}
              onChange={(e) => setConfirmDrop(e.target.value)}
              data-testid='admin-org-oltp-confirm-drop'
            />
            <Button
              size='sm'
              variant='destructive'
              className='h-7 shrink-0 px-2 text-xs'
              disabled={confirmDrop !== data.database || deprovision.isPending}
              onClick={() => {
                deprovision.mutate();
                setConfirmDrop("");
              }}
              data-testid='admin-org-oltp-deprovision'
            >
              {deprovision.isPending ? (
                <Loader2 className='size-3 animate-spin' />
              ) : (
                <Trash2 className='size-3' />
              )}
              Deprovision
            </Button>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
};
