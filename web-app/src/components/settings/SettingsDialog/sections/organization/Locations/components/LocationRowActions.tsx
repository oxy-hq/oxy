import { Loader2, MoreHorizontal } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { useUpdateLocation } from "@/hooks/api/organizations";
import { apiErrorMessage } from "@/libs/apiError";
import type { LocationRow } from "@/types/operatingGraph";
import { LocationDialog } from "./LocationDialog";
import { LocationPeopleDialog } from "./LocationPeopleDialog";

/**
 * Everything an admin does to one location. Archive asks first: it takes the
 * place out of every picker, though the row stays as history.
 */
export function LocationRowActions({
  orgId,
  location,
  locations
}: {
  orgId: string;
  location: LocationRow;
  locations: LocationRow[];
}) {
  const [editOpen, setEditOpen] = useState(false);
  const [peopleOpen, setPeopleOpen] = useState(false);
  const [archiveOpen, setArchiveOpen] = useState(false);
  const update = useUpdateLocation();
  const closed = location.status === "archived" || location.status === "terminated";

  const handleArchive = () => {
    update.mutate(
      { orgId, locationId: location.id, request: { status: "archived" } },
      {
        onSuccess: () => {
          toast.success(`Archived ${location.name}`);
          setArchiveOpen(false);
        },
        onError: (err) => toast.error(apiErrorMessage(err, "Couldn't archive the location"))
      }
    );
  };

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={update.isPending}
            data-testid={`settings-locations-menu-${location.id}`}
          >
            {update.isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-44'>
          <DropdownMenuItem onClick={() => setEditOpen(true)} data-testid='settings-locations-edit'>
            Edit…
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => setPeopleOpen(true)}
            data-testid='settings-locations-people'
          >
            People…
          </DropdownMenuItem>
          {!closed && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='text-destructive focus:text-destructive'
                onClick={() => setArchiveOpen(true)}
                data-testid='settings-locations-archive'
              >
                Archive
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <LocationDialog
        open={editOpen}
        onOpenChange={setEditOpen}
        orgId={orgId}
        locations={locations}
        location={location}
      />
      <LocationPeopleDialog
        open={peopleOpen}
        onOpenChange={setPeopleOpen}
        orgId={orgId}
        location={location}
      />

      <AlertDialog open={archiveOpen} onOpenChange={setArchiveOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Archive {location.name}?</AlertDialogTitle>
            <AlertDialogDescription>
              It stops being offered when you assign people or place a kiosk. Its assignments,
              external ids and history are kept, and you can reopen it from Edit.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
              onClick={(e) => {
                e.preventDefault();
                handleArchive();
              }}
              disabled={update.isPending}
              data-testid='settings-locations-archive-confirm'
            >
              {update.isPending && <Loader2 className='h-4 w-4 animate-spin' />}
              Archive
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
