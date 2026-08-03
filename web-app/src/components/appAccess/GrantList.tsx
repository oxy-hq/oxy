import { Lock, User, Users, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { GrantRole } from "@/types/appAccess";

export interface GrantRow {
  kind: "user" | "team";
  id: string;
  role: GrantRole;
  name: string;
  /** Headcount for a team, email for a person. */
  detail: string | null;
}

/**
 * The access list itself.
 *
 * Teams and people are one list, not two: they answer the same question, and
 * splitting them would make an admin check two places to know who can get in. The
 * icon carries the distinction.
 */
export function GrantList({
  rows,
  restricted,
  onRoleChange,
  onRemove
}: {
  rows: GrantRow[];
  /** Whether the app is restricted — decides what an EMPTY list means. */
  restricted: boolean;
  onRoleChange: (kind: "user" | "team", id: string, role: GrantRole) => void;
  onRemove: (kind: "user" | "team", id: string) => void;
}) {
  if (rows.length === 0) {
    // The two branches mean opposite things, so they can't share copy. On a
    // restricted app an empty list locks everyone out; on an open one it is the
    // normal, healthy state — "Nobody can open this app yet" would be flatly untrue
    // when everyone can.
    return (
      <div className='flex flex-col items-center gap-1.5 rounded-lg border border-dashed px-6 py-8 text-center'>
        <Lock className='size-5 text-muted-foreground' aria-hidden />
        <p className='font-medium text-sm'>
          {restricted ? "Nobody can open this app yet" : "No extra roles"}
        </p>
        <p className='max-w-sm text-muted-foreground text-xs leading-relaxed'>
          {restricted
            ? "Add a team or a person above. Organization owners and admins can always open it, so you can't lock yourself out."
            : "Everyone in the organization can open this app. Add someone here only to give them its admin surface without making them an organization admin."}
        </p>
      </div>
    );
  }

  return (
    <ul className='divide-y rounded-lg border'>
      {rows.map((row) => {
        const Icon = row.kind === "team" ? Users : User;
        return (
          <li key={`${row.kind}:${row.id}`} className='flex items-center gap-3 px-3 py-2.5'>
            <Icon className='size-4 shrink-0 text-muted-foreground' aria-hidden />
            <div className='min-w-0 flex-1'>
              <p className='truncate font-medium text-sm'>{row.name}</p>
              {row.detail && <p className='truncate text-muted-foreground text-xs'>{row.detail}</p>}
            </div>

            <Select
              value={row.role}
              onValueChange={(v) => onRoleChange(row.kind, row.id, v as GrantRole)}
            >
              <SelectTrigger className='h-8 w-32 shrink-0' aria-label={`Role for ${row.name}`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='member'>Can open</SelectItem>
                <SelectItem value='admin'>Can administer</SelectItem>
              </SelectContent>
            </Select>

            <Button
              variant='ghost'
              size='icon'
              className='size-8 shrink-0 text-muted-foreground hover:text-destructive'
              onClick={() => onRemove(row.kind, row.id)}
              aria-label={`Remove ${row.name}`}
            >
              <X className='size-4' aria-hidden />
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
