import { Plus, User, Users } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "@/components/ui/shadcn/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import type { GrantablePerson, Team } from "@/types/appAccess";

/**
 * Adds a team or a person to the access list.
 *
 * Teams come first and are labelled with their headcount, because granting a team
 * is the choice that scales — a per-person grant is the exception, and the ordering
 * says so without a paragraph explaining it.
 *
 * Anyone already on the list is filtered out rather than shown disabled: a
 * shrinking list of things you can still add is easier to scan than a list where
 * most rows do nothing.
 */
export function GrantPicker({
  teams,
  people,
  peopleUnavailable = false,
  alreadyGranted,
  onAdd
}: {
  teams: Team[];
  people: GrantablePerson[];
  /** The people list failed to load — show why instead of an empty group. */
  peopleUnavailable?: boolean;
  alreadyGranted: Set<string>;
  onAdd: (kind: "user" | "team", id: string) => void;
}) {
  const [open, setOpen] = useState(false);

  const availableTeams = teams.filter((t) => !alreadyGranted.has(`team:${t.id}`));
  const availablePeople = people.filter((p) => !alreadyGranted.has(`user:${p.user_id}`));
  const nothingLeft = availableTeams.length === 0 && availablePeople.length === 0;

  const handleAdd = (kind: "user" | "team", id: string) => {
    onAdd(kind, id);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant='outline' size='sm' className='w-fit' disabled={nothingLeft}>
          <Plus className='size-4' aria-hidden />
          Add team or person
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-[--radix-popover-trigger-width] min-w-80 p-0' align='start'>
        <Command>
          <CommandInput placeholder='Search teams and people…' />
          <CommandList>
            <CommandEmpty>No match.</CommandEmpty>

            {availableTeams.length > 0 && (
              <CommandGroup heading='Teams'>
                {availableTeams.map((team) => (
                  <CommandItem
                    key={team.id}
                    value={`team ${team.name}`}
                    onSelect={() => handleAdd("team", team.id)}
                    className='gap-2'
                  >
                    <Users className='size-4 shrink-0 text-muted-foreground' aria-hidden />
                    <span className='truncate'>{team.name}</span>
                    <span className='ml-auto shrink-0 text-muted-foreground text-xs'>
                      {team.member_count} {team.member_count === 1 ? "person" : "people"}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            )}

            {peopleUnavailable && (
              <CommandGroup heading='People'>
                <CommandItem disabled className='text-muted-foreground text-xs'>
                  Couldn't load people — teams only
                </CommandItem>
              </CommandGroup>
            )}

            {!peopleUnavailable && availablePeople.length > 0 && (
              <CommandGroup heading='People'>
                {availablePeople.map((person) => (
                  <CommandItem
                    key={person.user_id}
                    value={`person ${person.name} ${person.email}`}
                    onSelect={() => handleAdd("user", person.user_id)}
                    className='gap-2'
                  >
                    <User className='size-4 shrink-0 text-muted-foreground' aria-hidden />
                    <span className='truncate'>{person.name}</span>
                    <span className='ml-auto shrink-0 truncate text-muted-foreground text-xs'>
                      {person.email}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
