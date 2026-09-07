import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { useCreateAssignment, useDeleteAssignment, usePeople } from "@/hooks/api/organizations";
import { apiErrorMessage } from "@/libs/apiError";
import type { AssignmentRow, RoleRow } from "@/types/operatingGraph";
import { AssignmentList } from "../../shared/AssignmentList";
import { NO_PERSON, PersonSelect } from "../../shared/PersonSelect";

/**
 * The org-wide positions and who holds each: an area manager is held across
 * the whole organization, so there is no location dialog to reach it from.
 * Every location-scoped position is edited from its place instead.
 */
export function OrgWidePane({
  orgId,
  roles,
  assignments
}: {
  orgId: string;
  roles: RoleRow[];
  assignments: AssignmentRow[];
}) {
  const orgWide = roles.filter((r) => r.scope === "franchisor");
  const { people } = usePeople(orgId);

  return (
    <div className='flex flex-col gap-3'>
      <div>
        <h3 className='font-medium'>Org-wide positions</h3>
        <p className='text-muted-foreground text-xs'>
          Held across the whole organization, not at one place. Who holds a location position is set
          from Locations.
        </p>
      </div>
      {orgWide.length === 0 ? (
        <p className='rounded-md border py-6 text-center text-muted-foreground text-sm'>
          No org-wide positions yet. Create one with the Org-wide scope, like Area manager.
        </p>
      ) : (
        <div className='flex flex-col gap-4'>
          {orgWide.map((role) => (
            <OrgWideHolders
              key={role.id}
              orgId={orgId}
              role={role}
              assignments={assignments.filter((a) => a.role_id === role.id)}
              people={people}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function OrgWideHolders({
  orgId,
  role,
  assignments,
  people
}: {
  orgId: string;
  role: RoleRow;
  assignments: AssignmentRow[];
  people: ReturnType<typeof usePeople>["people"];
}) {
  const createAssignment = useCreateAssignment();
  const deleteAssignment = useDeleteAssignment();
  const [personId, setPersonId] = useState("");
  const [supervisorId, setSupervisorId] = useState(NO_PERSON);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!personId || createAssignment.isPending) return;
    setError(null);
    try {
      const created = await createAssignment.mutateAsync({
        orgId,
        request: {
          user_id: personId,
          role_id: role.id,
          location_id: null,
          supervisor_id: supervisorId === NO_PERSON ? null : supervisorId
        }
      });
      toast.success(`Added ${created.user_name} as ${role.name}`);
      setPersonId("");
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
        onSuccess: () => toast.success(`Removed ${assignment.user_name} as ${role.name}`),
        onError: (err) => toast.error(apiErrorMessage(err, "Couldn't remove that assignment")),
        onSettled: () => setRemovingId(null)
      }
    );
  };

  return (
    <div
      className='flex flex-col gap-3 rounded-md border p-3'
      data-testid={`settings-positions-holders-${role.id}`}
    >
      <p className='font-medium text-sm'>{role.name}</p>
      <AssignmentList
        assignments={assignments}
        columns={["person", "supervisor"]}
        onRemove={handleRemove}
        removingId={removingId}
        testIdPrefix='settings-positions-holder'
        emptyText='Nobody holds it yet.'
      />
      <form onSubmit={handleAdd} className='flex flex-col gap-2 sm:flex-row sm:items-start'>
        <div className='flex-1'>
          <PersonSelect
            people={people}
            value={personId}
            onValueChange={(v) => {
              setPersonId(v);
              if (v === supervisorId) setSupervisorId(NO_PERSON);
            }}
            placeholder='Add a person'
            testId={`settings-positions-holder-person-${role.id}`}
          />
        </div>
        <div className='flex-1'>
          <PersonSelect
            people={people}
            value={supervisorId}
            onValueChange={setSupervisorId}
            allowNone
            noneLabel='Reports to nobody'
            exclude={personId ? [personId] : []}
            placeholder='Reports to'
            testId={`settings-positions-holder-supervisor-${role.id}`}
          />
        </div>
        <Button
          type='submit'
          size='sm'
          variant='outline'
          disabled={!personId || createAssignment.isPending}
          data-testid={`settings-positions-holder-add-${role.id}`}
        >
          {createAssignment.isPending ? "Adding..." : "Add"}
        </Button>
      </form>
      {error && <p className='text-destructive text-sm'>{error}</p>}
    </div>
  );
}
