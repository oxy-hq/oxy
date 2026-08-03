import { isAxiosError } from "axios";
import { Loader2, Plus, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "@/components/ui/shadcn/command";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Separator } from "@/components/ui/shadcn/separator";
import {
  useAddTeamMember,
  useCreateTeam,
  useRemoveTeamMember,
  useTeam,
  useUpdateTeam
} from "@/hooks/api/appAccess";
import { useOrgMembers } from "@/hooks/api/organizations";
import type { Team } from "@/types/appAccess";

/**
 * Create or edit one team.
 *
 * Membership edits save immediately — an admin adding six people shouldn't have to
 * remember to press Save, and each add is independently meaningful. The name and
 * description are a form because they're one coherent edit.
 *
 * A team can only ever contain members of the org; the picker offers nothing else,
 * and the server rejects anything else.
 */
export function TeamEditor({
  orgId,
  team,
  open,
  onOpenChange
}: {
  orgId: string;
  team: Team | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const isEditing = team !== null;
  const { data: detail } = useTeam(orgId, open && team ? team.id : null);
  const { data: orgMembers } = useOrgMembers(orgId, open);

  const createTeam = useCreateTeam();
  const updateTeam = useUpdateTeam();
  const addMember = useAddTeamMember();
  const removeMember = useRemoveTeamMember();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(team?.name ?? "");
    setDescription(team?.description ?? "");
    setError(null);
  }, [open, team]);

  const members = detail?.members ?? [];
  const candidates = useMemo(() => {
    const inTeam = new Set(members.map((m) => m.user_id));
    return (orgMembers ?? []).filter((m) => !inTeam.has(m.user_id));
  }, [orgMembers, members]);

  const handleSaveDetails = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Give the team a name.");
      return;
    }
    setError(null);
    try {
      if (isEditing) {
        await updateTeam.mutateAsync({
          orgId,
          teamId: team.id,
          name: trimmed,
          description: description.trim() || null
        });
        toast.success("Team updated");
      } else {
        await createTeam.mutateAsync({
          orgId,
          name: trimmed,
          description: description.trim() || null
        });
        toast.success(`Created ${trimmed}`);
      }
      onOpenChange(false);
    } catch (err) {
      if (isAxiosError(err) && err.response?.status === 409) {
        setError("A team with that name already exists.");
        return;
      }
      setError("Couldn't save the team.");
    }
  };

  const handleAdd = async (userId: string) => {
    if (!team) return;
    setPickerOpen(false);
    try {
      await addMember.mutateAsync({ orgId, teamId: team.id, userId });
    } catch {
      toast.error("Couldn't add them to the team");
    }
  };

  const handleRemove = async (userId: string, label: string) => {
    if (!team) return;
    try {
      await removeMember.mutateAsync({ orgId, teamId: team.id, userId });
    } catch {
      toast.error(`Couldn't remove ${label}`);
    }
  };

  const saving = createTeam.isPending || updateTeam.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>{isEditing ? `Edit ${team.name}` : "New team"}</DialogTitle>
          <DialogDescription>
            {isEditing
              ? "Rename the team or change who's in it."
              : "Name the team first. You can add people once it exists."}
          </DialogDescription>
        </DialogHeader>

        <div className='flex flex-col gap-4'>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='team-name'>Name</Label>
            <Input
              id='team-name'
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder='Finance'
              autoFocus
            />
          </div>

          <div className='flex flex-col gap-2'>
            <Label htmlFor='team-description'>Description (optional)</Label>
            <Input
              id='team-description'
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder='Who this group is, so the next admin knows'
            />
          </div>

          {isEditing && (
            <>
              <Separator />
              <div className='flex flex-col gap-2'>
                <div className='flex items-center justify-between'>
                  <Label>People ({members.length})</Label>
                  <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
                    <PopoverTrigger asChild>
                      <Button variant='outline' size='sm' disabled={candidates.length === 0}>
                        <Plus className='size-4' aria-hidden />
                        Add person
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent className='w-72 p-0' align='end'>
                      <Command>
                        <CommandInput placeholder='Search organization members…' />
                        <CommandList>
                          <CommandEmpty>Everyone is already on this team.</CommandEmpty>
                          <CommandGroup>
                            {candidates.map((m) => (
                              <CommandItem
                                key={m.user_id}
                                value={`${m.name} ${m.email}`}
                                onSelect={() => handleAdd(m.user_id)}
                                className='gap-2'
                              >
                                <span className='truncate'>{m.name || m.email}</span>
                                <span className='ml-auto shrink-0 truncate text-muted-foreground text-xs'>
                                  {m.email}
                                </span>
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>
                </div>

                {members.length === 0 ? (
                  <p className='rounded-md border border-dashed px-3 py-6 text-center text-muted-foreground text-xs'>
                    Nobody yet. A team with no people grants nothing.
                  </p>
                ) : (
                  <ul className='max-h-52 divide-y overflow-y-auto rounded-md border'>
                    {members.map((m) => (
                      <li key={m.user_id} className='flex items-center gap-2 px-3 py-2'>
                        <div className='min-w-0 flex-1'>
                          <p className='truncate text-sm'>{m.name}</p>
                          <p className='truncate text-muted-foreground text-xs'>{m.email}</p>
                        </div>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='size-7 text-muted-foreground hover:text-destructive'
                          onClick={() => handleRemove(m.user_id, m.name)}
                          aria-label={`Remove ${m.name} from the team`}
                        >
                          <X className='size-4' aria-hidden />
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </>
          )}

          {error && (
            <p role='alert' className='text-destructive text-sm'>
              {error}
            </p>
          )}
        </div>

        <div className='flex justify-end gap-2'>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            {isEditing ? "Done" : "Cancel"}
          </Button>
          <Button onClick={handleSaveDetails} disabled={saving}>
            {saving && <Loader2 className='size-4 animate-spin' aria-hidden />}
            {isEditing ? "Save changes" : "Create team"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
