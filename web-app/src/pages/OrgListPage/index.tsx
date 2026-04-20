import { MailPlus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import CreateOrgDialog from "@/components/org/CreateOrgDialog";
import JoinOrgDialog from "@/components/org/JoinOrgDialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgs } from "@/hooks/api/organizations";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useTheme from "@/stores/useTheme";
import type { Organization } from "@/types/organization";
import NewOrgCard from "./components/NewOrgCard";
import OrgCard from "./components/OrgCard";

export default function OrgListPage() {
  const navigate = useNavigate();
  const { data: orgs, isPending } = useOrgs();
  const { setOrg } = useCurrentOrg();
  const { theme } = useTheme();
  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);
  const [query, setQuery] = useState("");

  const filteredOrgs = useMemo(() => {
    if (!orgs) return [];
    const q = query.trim().toLowerCase();
    if (!q) return orgs;
    return orgs.filter((o) => o.name.toLowerCase().includes(q));
  }, [orgs, query]);

  if (isPending) {
    return (
      <div className='flex min-h-screen items-center justify-center bg-background'>
        <Spinner className='size-6' />
      </div>
    );
  }

  const enterOrg = (org: Organization) => {
    setOrg(org);
    navigate(ROUTES.ORG(org.slug).WORKSPACES);
  };

  const handleCreated = (org: Organization) => {
    setCreateOpen(false);
    toast.success(`Organization "${org.name}" created`);
    enterOrg(org);
  };

  const handleJoined = (org: Organization) => {
    setJoinOpen(false);
    toast.success(`Joined "${org.name}"`);
    enterOrg(org);
  };

  return (
    <div className='grid h-full w-full overflow-auto'>
      <div className='flex flex-col gap-4 p-6 md:p-10'>
        <div className='flex items-center gap-2 self-center font-medium md:self-start'>
          <img src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"} alt='Oxy' />
          <span className='truncate text-sm'>Oxygen</span>
        </div>

        <div className='mx-auto w-full max-w-4xl py-12'>
          <h1 className='mb-6 font-semibold text-2xl tracking-tight'>Organizations</h1>

          <div className='mb-8 flex items-center justify-between gap-4'>
            <div className='relative w-full max-w-sm'>
              <Search className='absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground' />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder='Search for an organization'
                className='pl-9'
              />
            </div>
            <Button variant='outline' onClick={() => setJoinOpen(true)}>
              <MailPlus className='h-3.5 w-3.5' />
              Join org
            </Button>
          </div>

          <ul className='grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3'>
            {filteredOrgs.map((org, index) => (
              <OrgCard key={org.id} org={org} index={index} onSelect={() => enterOrg(org)} />
            ))}
            <NewOrgCard index={filteredOrgs.length} onClick={() => setCreateOpen(true)} />
          </ul>
        </div>
      </div>

      <CreateOrgDialog open={createOpen} onOpenChange={setCreateOpen} onCreated={handleCreated} />
      <JoinOrgDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={handleJoined} />
    </div>
  );
}
