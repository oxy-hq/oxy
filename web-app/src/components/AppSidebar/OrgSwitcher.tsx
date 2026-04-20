import { Check, ChevronsUpDown, Plus, UserPlus } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import CreateOrgDialog from "@/components/org/CreateOrgDialog";
import JoinOrgDialog from "@/components/org/JoinOrgDialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { useOrgs } from "@/hooks/api/organizations";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { Organization } from "@/types/organization";

export default function OrgSwitcher() {
  const navigate = useNavigate();
  const { data: orgs } = useOrgs();
  const { org: currentOrg, setOrg } = useCurrentOrg();
  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);

  const handleSwitch = (org: Organization) => {
    setOrg(org);
    navigate(ROUTES.ORG(org.slug).WORKSPACES);
  };

  const handleOrgCreated = (org: Organization) => {
    setCreateOpen(false);
    setOrg(org);
    toast.success(`Organization "${org.name}" created`);
    navigate(ROUTES.ORG(org.slug).WORKSPACES);
  };

  const handleOrgJoined = (org: Organization) => {
    setJoinOpen(false);
    setOrg(org);
    toast.success(`Joined "${org.name}"`);
    navigate(ROUTES.ORG(org.slug).WORKSPACES);
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type='button'
            className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted'
          >
            <div className='flex h-6 w-6 items-center justify-center rounded bg-primary font-bold text-primary-foreground text-xs'>
              {currentOrg?.name?.[0]?.toUpperCase() ?? "?"}
            </div>
            <span className='flex-1 truncate text-left font-medium'>
              {currentOrg?.name ?? "Select organization"}
            </span>
            <ChevronsUpDown className='h-4 w-4 text-muted-foreground' />
          </button>
        </DropdownMenuTrigger>

        <DropdownMenuContent align='start' className='w-64'>
          {orgs?.map((org) => (
            <DropdownMenuItem
              key={org.id}
              onClick={() => handleSwitch(org)}
              className={cn("flex items-center gap-2", currentOrg?.id === org.id && "bg-muted")}
            >
              <div className='flex h-6 w-6 items-center justify-center rounded bg-primary/10 font-bold text-primary text-xs'>
                {org.name[0]?.toUpperCase()}
              </div>
              <span className='flex-1 truncate'>{org.name}</span>
              {currentOrg?.id === org.id && <Check className='h-4 w-4 text-primary' />}
            </DropdownMenuItem>
          ))}

          <DropdownMenuSeparator />

          <DropdownMenuItem onClick={() => setCreateOpen(true)}>
            <Plus className='mr-2 h-4 w-4' />
            Create organization
          </DropdownMenuItem>

          <DropdownMenuItem onClick={() => setJoinOpen(true)}>
            <UserPlus className='mr-2 h-4 w-4' />
            Join organization
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <CreateOrgDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={handleOrgCreated}
      />
      <JoinOrgDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={handleOrgJoined} />
    </>
  );
}
