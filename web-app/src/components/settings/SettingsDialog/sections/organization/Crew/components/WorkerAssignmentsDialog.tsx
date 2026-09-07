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
  useLocations,
  useOrgRoles,
  usePeople
} from "@/hooks/api/organizations";
import { apiErrorMessage } from "@/libs/apiError";
import type { FrontlineWorker } from "@/types/frontline";
import type { AssignmentRow } from "@/types/operatingGraph";
import { AssignmentList } from "../../shared/AssignmentList";
import { LocationSelect } from "../../shared/LocationSelect";
import { NO_PERSON, PersonSelect } from "../../shared/PersonSelect";
import { PositionSelect } from "../../shared/PositionSelect";

export function WorkerAssignmentsDialog({
  open,
  onOpenChange,
  orgId,
  worker
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
  worker: FrontlineWorker;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>Where {worker.name} works</DialogTitle>
          <DialogDescription>
            The positions they hold and where. A position grants no apps; those are set under Apps.
          </DialogDescription>
        </DialogHeader>
        <WorkerAssignments orgId={orgId} worker={worker} />
      </DialogContent>
    </Dialog>
  );
}

function WorkerAssignments({ orgId, worker }: { orgId: string; worker: FrontlineWorker }) {
  // The full rows, not the worker's own `assignments[]`: that copy has no
  // supervisor name, and this dialog shows who they report to.
  const assignments = useAssignments(orgId, { user_id: worker.user_id });
  const roles = useOrgRoles(orgId);
  const locations = useLocations(orgId);
  const { people } = usePeople(orgId);
  const createAssignment = useCreateAssignment();
  const deleteAssignment = useDeleteAssignment();

  const [roleId, setRoleId] = useState("");
  const [locationId, setLocationId] = useState("");
  const [supervisorId, setSupervisorId] = useState(NO_PERSON);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const role = roles.data?.find((r) => r.id === roleId);
  const needsLocation = role?.scope === "location";
  const canAdd = roleId.length > 0 && (!needsLocation || locationId.length > 0);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canAdd || createAssignment.isPending) return;
    setError(null);
    try {
      const created = await createAssignment.mutateAsync({
        orgId,
        request: {
          user_id: worker.user_id,
          role_id: roleId,
          location_id: needsLocation ? locationId : null,
          supervisor_id: supervisorId === NO_PERSON ? null : supervisorId
        }
      });
      toast.success(`Added ${created.role_name} for ${worker.name}`);
      setRoleId("");
      setLocationId("");
      setSupervisorId(NO_PERSON);
    } catch (err) {
      setError(apiErrorMessage(err, "Couldn't add that assignment"));
    }
  };

  const handleRemove = (assignment: AssignmentRow) => {
    setRemovingId(assignment.id);
    deleteAssignment.mutate(
      { orgId, assignmentId: assignment.id },
      {
        onSuccess: () => toast.success(`Removed ${assignment.role_name} for ${worker.name}`),
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
          <span className='sr-only'>Loading assignments</span>
        </div>
      ) : assignments.isError ? (
        <p className='text-destructive text-sm'>Failed to load their assignments.</p>
      ) : (
        <AssignmentList
          assignments={assignments.data}
          columns={["position", "place", "supervisor"]}
          onRemove={handleRemove}
          removingId={removingId}
          testIdPrefix='settings-crew-assignment'
          emptyText='No assignments yet. Add the first one below.'
        />
      )}

      <form onSubmit={handleAdd} className='flex flex-col gap-3 rounded-md border p-3'>
        <p className='font-medium text-sm'>Add assignment</p>
        <div className='grid gap-3 sm:grid-cols-2'>
          <div className='space-y-1.5'>
            <Label htmlFor='worker-assignment-position'>Position</Label>
            <PositionSelect
              id='worker-assignment-position'
              roles={roles.data ?? []}
              value={roleId}
              onValueChange={setRoleId}
              testId='settings-crew-assignment-position'
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='worker-assignment-location'>Location</Label>
            <LocationSelect
              id='worker-assignment-location'
              locations={locations.data ?? []}
              value={needsLocation ? locationId : ""}
              onValueChange={setLocationId}
              disabled={!needsLocation}
              placeholder={role && !needsLocation ? "Org-wide" : "Pick a location"}
              testId='settings-crew-assignment-location'
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
            exclude={[worker.user_id]}
            placeholder='Nobody'
            testId='settings-crew-assignment-supervisor'
          />
        </div>
        {error && <p className='text-destructive text-sm'>{error}</p>}
        <div className='flex justify-end'>
          <Button
            type='submit'
            size='sm'
            disabled={!canAdd || createAssignment.isPending}
            data-testid='settings-crew-assignment-add'
          >
            {createAssignment.isPending ? "Adding..." : "Add assignment"}
          </Button>
        </div>
      </form>
    </div>
  );
}
