import { Loader2, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { assignmentPlace, PERSON_KIND_MARK } from "@/libs/operatingGraph";
import type { AssignmentRow } from "@/types/operatingGraph";

export type AssignmentColumn = "person" | "position" | "place" | "supervisor";

/**
 * The assignments one dialog is about, each with a remove control. Which
 * columns show depends on what the dialog has already fixed: a location's
 * people dialog needs no place column, a worker's needs no person.
 */
export function AssignmentList({
  assignments,
  columns,
  onRemove,
  removingId,
  testIdPrefix,
  emptyText
}: {
  assignments: AssignmentRow[];
  columns: AssignmentColumn[];
  onRemove: (assignment: AssignmentRow) => void;
  removingId?: string | null;
  /** `settings-<section>-<noun>`; the row and its remove button hang off it by id. */
  testIdPrefix: string;
  emptyText: string;
}) {
  if (assignments.length === 0) {
    return (
      <p className='rounded-md border border-dashed px-3 py-3 text-center text-muted-foreground text-xs'>
        {emptyText}
      </p>
    );
  }
  const show = new Set(columns);
  return (
    <ul className='max-h-64 divide-y overflow-y-auto rounded-md border'>
      {assignments.map((a) => {
        const mark = PERSON_KIND_MARK[a.user_kind];
        const removing = removingId === a.id;
        return (
          <li
            key={a.id}
            className='flex items-center gap-3 px-3 py-2 text-sm'
            data-testid={`${testIdPrefix}-row-${a.id}`}
          >
            <div className='flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2 gap-y-0.5'>
              {show.has("person") && (
                <span className='flex items-baseline gap-1.5'>
                  <span className='font-medium'>{a.user_name}</span>
                  {mark && (
                    <span className='rounded-sm bg-muted px-1 text-muted-foreground text-xs'>
                      {mark}
                    </span>
                  )}
                </span>
              )}
              {show.has("position") && <span>{a.role_name}</span>}
              {show.has("place") && (
                <span className='text-muted-foreground'>{assignmentPlace(a)}</span>
              )}
              {show.has("supervisor") && a.supervisor_name && (
                <span className='text-muted-foreground text-xs'>
                  reports to {a.supervisor_name}
                </span>
              )}
            </div>
            <Button
              type='button'
              variant='ghost'
              size='icon'
              className='h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive'
              onClick={() => onRemove(a)}
              disabled={removing}
              aria-label={`Remove ${a.user_name} as ${a.role_name}`}
              data-testid={`${testIdPrefix}-remove-${a.id}`}
            >
              {removing ? <Loader2 className='h-4 w-4 animate-spin' /> : <X className='h-4 w-4' />}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
