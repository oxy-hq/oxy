import { Loader2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Label } from "@/components/ui/shadcn/label";
import {
  useAssignments,
  useCreateAssignment,
  useDeleteAssignment,
  useOrgRoles,
  usePeople
} from "@/hooks/api/organizations";
import { apiErrorMessage } from "@/libs/apiError";
import type { AssignmentRow, LocationRow } from "@/types/operatingGraph";
import { AssignmentList } from "../../shared/AssignmentList";
import { NO_PERSON, PersonSelect } from "../../shared/PersonSelect";
import { PositionSelect } from "../../shared/PositionSelect";

export function LocationPeopleDialog({
  open,
  onOpenChange,
  orgId,
  location
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
  location: LocationRow;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>People at {location.name}</DialogTitle>
          <DialogDescription>
            Who holds a position here. A position says what someone is called and what work routes
            to them; it grants no permissions.
          </DialogDescription>
        </DialogHeader>
        <LocationPeople orgId={orgId} location={location} />
      </DialogContent>
    </Dialog>
  );
}

function LocationPeople({ orgId, location }: { orgId: string; location: LocationRow }) {
  const assignments = useAssignments(orgId, { location_id: location.id });
  const roles = useOrgRoles(orgId);
  const { people } = usePeople(orgId);
  const createAssignment = useCreateAssignment();
  const deleteAssignment = useDeleteAssignment();

  const [personId, setPersonId] = useState("");
  const [roleId, setRoleId] = useState("");
  const [supervisorId, setSupervisorId] = useState(NO_PERSON);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canAdd = personId.length > 0 && roleId.length > 0;

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canAdd || createAssignment.isPending) return;
    setError(null);
    try {
      const created = await createAssignment.mutateAsync({
        orgId,
        request: {
          user_id: personId,
          role_id: roleId,
          location_id: location.id,
          supervisor_id: supervisorId === NO_PERSON ? null : supervisorId
        }
      });
      toast.success(`Added ${created.user_name} as ${created.role_name}`);
      setPersonId("");
      setRoleId("");
      setSupervisorId(NO_PERSON);
    } catch (err) {
      setError(apiErrorMessage(err, "Couldn't add that person"));
    }
  };

  const handleRemove = (assignment: AssignmentRow) => {
    setRemovingId(assignment.id);
    deleteAssignment.mutate(
      { orgId, assignmentId: assignment.id },
      {
        onSuccess: () =>
          toast.success(`Removed ${assignment.user_name} as ${assignment.role_name}`),
        onError: (err) => toast.error(apiErrorMessage(err, "Couldn't remove that assignment")),
        onSettled: () => setRemovingId(null)
      }
    );
  };

  return (
    <div className='flex flex-col gap-4 pt-1'>
      {assignments.isPending ? (
        <div className='flex min-h-16 items-center justify-center'>
          <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
          <span className='sr-only'>Loading people</span>
        </div>
      ) : assignments.isError ? (
        <p className='text-destructive text-sm'>Failed to load who works here.</p>
      ) : (
        <AssignmentList
          assignments={assignments.data}
          columns={["person", "position", "supervisor"]}
          onRemove={handleRemove}
          removingId={removingId}
          testIdPrefix='settings-locations-people'
          emptyText='Nobody is assigned here yet. Add the first person below.'
        />
      )}

      <form onSubmit={handleAdd} className='flex flex-col gap-3 rounded-md border p-3'>
        <p className='font-medium text-sm'>Add person</p>
        <div className='grid gap-3 sm:grid-cols-2'>
          <div className='space-y-1.5'>
            <Label>Person</Label>
            <PersonSelect
              people={people}
              value={personId}
              onValueChange={(v) => {
                setPersonId(v);
                if (v === supervisorId) setSupervisorId(NO_PERSON);
              }}
              testId='settings-locations-people-person'
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='location-people-position'>Position</Label>
            <PositionSelect
              id='location-people-position'
              roles={roles.data ?? []}
              scope='location'
              value={roleId}
              onValueChange={setRoleId}
              testId='settings-locations-people-position'
            />
          </div>
        </div>
        <div className='space-y-1.5'>
          <Label>Reports to</Label>
          <PersonSelect
            people={people}
            value={supervisorId}
            onValueChange={setSupervisorId}
            allowNone
            exclude={personId ? [personId] : []}
            placeholder='Nobody'
            testId='settings-locations-people-supervisor'
          />
        </div>
        {error && <p className='text-destructive text-sm'>{error}</p>}
        <div className='flex justify-end'>
          <Button
            type='submit'
            size='sm'
            disabled={!canAdd || createAssignment.isPending}
            data-testid='settings-locations-people-add'
          >
            {createAssignment.isPending ? "Adding..." : "Add person"}
          </Button>
        </div>
      </form>
    </div>
  );
}
