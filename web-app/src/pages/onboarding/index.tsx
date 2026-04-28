import { useQueryClient } from "@tanstack/react-query";
import { Building2, MailPlus } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import CreateOrgDialog from "@/components/org/CreateOrgDialog";
import JoinOrgDialog from "@/components/org/JoinOrgDialog";
import { Card, CardContent, CardDescription, CardTitle } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useMyInvitations } from "@/hooks/api/organizations";
import queryKeys from "@/hooks/api/queryKey";
import ROUTES from "@/libs/utils/routes";
import type { Organization } from "@/types/organization";
import OnboardingHeader from "./components/OnboardingHeader";
import PendingInvitesCard from "./components/PendingInvitesCard";

/**
 * Post-login entry point for users with zero orgs. Runs an invite-check first:
 *
 *   has pending invite(s) → "You've been invited" screen (accept → dispatcher
 *                           sends them straight into a workspace, or "Create
 *                           my own org instead" continues to the dialog path)
 *   no invites            → welcome screen with Create / Join org dialogs
 *
 * Accepting an invite updates the orgs cache, so the dispatcher routed through
 * by navigating to ROOT can immediately pick the joined org.
 */
export default function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);
  const [dismissedInvites, setDismissedInvites] = useState(false);

  const { data: invites, isPending: invitesPending } = useMyInvitations();

  const handleCreated = (org: Organization) => {
    setCreateOpen(false);
    // Fresh-org path: show invite members before workspace creation.
    navigate(`${ROUTES.ORG(org.slug).ONBOARDING}?step=invite`, { replace: true });
  };

  const handleJoined = async (org: Organization) => {
    setJoinOpen(false);
    await queryClient.refetchQueries({ queryKey: queryKeys.org.list() });
    navigate(ROUTES.ORG(org.slug).ROOT, { replace: true });
  };

  if (invitesPending) {
    return (
      <div className='flex min-h-screen min-w-screen items-center justify-center bg-background'>
        <Spinner className='size-6' />
      </div>
    );
  }

  const hasInvites = !dismissedInvites && !!invites && invites.length > 0;

  return (
    <div className='flex min-h-screen w-full flex-col bg-background'>
      <OnboardingHeader />

      <div className='mx-auto flex w-full max-w-xl flex-1 flex-col items-center justify-center gap-8 px-6 pb-16'>
        {hasInvites ? (
          <PendingInvitesCard invites={invites} onCreateInstead={() => setDismissedInvites(true)} />
        ) : (
          <>
            <div className='flex flex-col items-center gap-2 text-center'>
              <h1 className='font-semibold text-2xl tracking-tight'>Welcome to Oxygen</h1>
              <p className='text-muted-foreground text-sm'>
                Create an organization for your team or join one you've been invited to.
              </p>
            </div>

            <div className='grid w-full grid-cols-1 gap-3 sm:grid-cols-2'>
              <OnboardingOption
                icon={<Building2 className='size-5 text-primary' />}
                title='Create organization'
                description='Start a new workspace and invite your team.'
                onClick={() => setCreateOpen(true)}
              />
              <OnboardingOption
                icon={<MailPlus className='size-5 text-primary' />}
                title='Join organization'
                description='Paste an invitation code or link from your team.'
                onClick={() => setJoinOpen(true)}
              />
            </div>
          </>
        )}
      </div>

      <CreateOrgDialog open={createOpen} onOpenChange={setCreateOpen} onCreated={handleCreated} />
      <JoinOrgDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={handleJoined} />
    </div>
  );
}

function OnboardingOption({
  icon,
  title,
  description,
  onClick
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button
      type='button'
      onClick={onClick}
      className='group text-left transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-ring'
    >
      <Card className='h-full transition-colors group-hover:border-primary/40'>
        <CardContent className='flex flex-col gap-3 p-5'>
          <div className='flex size-10 items-center justify-center rounded-lg bg-primary/10'>
            {icon}
          </div>
          <div className='flex flex-col gap-1'>
            <CardTitle className='text-base'>{title}</CardTitle>
            <CardDescription className='text-xs'>{description}</CardDescription>
          </div>
        </CardContent>
      </Card>
    </button>
  );
}
