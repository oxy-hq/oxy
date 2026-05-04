import { LogOut } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import JoinOrgDialog from "@/components/org/JoinOrgDialog";
import { Button } from "@/components/ui/shadcn/button";
import { useAuth } from "@/contexts/AuthContext";
import { useCreatePortalSession, useOrgBillingStatus } from "@/hooks/api/billing";
import { useOrgs } from "@/hooks/api/organizations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import type { BillingStatusId } from "@/services/api/billing";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { usePaywallStore } from "@/stores/usePaywallStore";
import useTheme from "@/stores/useTheme";

type PaywallVariant = Extract<BillingStatusId, "incomplete" | "unpaid" | "canceled">;

interface CopyVariant {
  title: string;
  description: string;
}

const ADMIN_COPY: Record<PaywallVariant, CopyVariant> = {
  incomplete: {
    title: "Thanks for signing up",
    description:
      "Our team will reach out within 24 hours to discuss pricing. Refresh once your sales call is complete to access the app."
  },
  unpaid: {
    title: "Your access is paused",
    description:
      "We weren't able to collect a recent payment. Update your payment method or contact your account team."
  },
  canceled: {
    title: "Your access is paused",
    description:
      "Your subscription has ended. Contact your account team to re-provision — your data is retained."
  }
};

const MEMBER_COPY: Record<PaywallVariant, CopyVariant> = {
  incomplete: {
    title: "Workspace is being set up",
    description: "Your owner needs to complete provisioning with Oxy. Please check back soon."
  },
  unpaid: {
    title: "This workspace is paused",
    description: "Ask your owner to update billing or contact the Oxy account team."
  },
  canceled: {
    title: "This workspace is paused",
    description: "Ask your owner to contact the Oxy account team to re-provision access."
  }
};

const SUPPORT_MAIL = "mailto:support@oxy.ai";

interface PaywallScreenProps {
  isAdmin: boolean;
}

/**
 * Full-screen paywall mounted by `OrgGuard` whenever the org's billing status
 * doesn't grant access (or after a 402 `subscription_required` from any API
 * call). Sales-gated — no pricing, no Subscribe CTA. Admins can update payment
 * method through the restricted Customer Portal when applicable; everyone
 * sees a "contact account team" link.
 */
export function PaywallScreen({ isAdmin }: PaywallScreenProps) {
  const status = usePaywallStore((s) => s.status) as PaywallVariant;
  const orgId = useCurrentOrg((s) => s.org?.id);
  const { data: billing } = useOrgBillingStatus(orgId ?? "", !!orgId);
  const { data: orgs } = useOrgs();
  const { data: currentUser } = useCurrentUser();
  const portal = useCreatePortalSession(orgId ?? "");
  const { logout } = useAuth();
  const navigate = useNavigate();
  const { theme } = useTheme();
  const [joinOpen, setJoinOpen] = useState(false);

  const copy = (isAdmin ? ADMIN_COPY : MEMBER_COPY)[status];
  const paymentActionUrl = billing?.payment_action_url ?? null;
  const showPortalCta = isAdmin && (status === "unpaid" || status === "canceled");
  const otherOrgs = (orgs ?? []).filter((org) => org.id !== orgId);

  return (
    <div className='flex min-h-screen w-full flex-col bg-background'>
      <div className='flex items-center justify-between gap-2 p-6 font-medium'>
        <div className='flex items-center gap-2'>
          <img src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"} alt='Oxy' />
          <span className='truncate text-sm'>Oxygen</span>
        </div>
        {currentUser?.email && (
          <div className='group relative'>
            <div className='flex cursor-pointer flex-col items-end text-right leading-tight'>
              <span className='text-muted-foreground text-xs'>Logged in as</span>
              <span className='truncate font-normal text-sm'>{currentUser.email}</span>
            </div>
            <div className='pointer-events-none absolute top-full right-0 z-10 pt-2 opacity-0 transition-opacity focus-within:pointer-events-auto focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100'>
              <Button
                variant='ghost'
                size='sm'
                onClick={logout}
                aria-label='Log out'
                className='gap-1.5 shadow-sm'
              >
                <LogOut className='size-3.5' />
                Log out
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className='flex flex-1 items-center justify-center px-6 pb-16'>
        <div className='w-full max-w-md space-y-6 rounded-2xl border bg-card p-8 text-center'>
          <div className='space-y-2'>
            <h1 className='font-serif text-2xl tracking-tight'>{copy.title}</h1>
            <p className='text-muted-foreground text-sm'>{copy.description}</p>
          </div>

          {otherOrgs.length > 0 ? (
            <div className='space-y-2 text-left'>
              <p className='text-muted-foreground text-xs uppercase tracking-wide'>
                Switch to another organization
              </p>
              <div className='space-y-1'>
                {otherOrgs.map((org) => (
                  <button
                    key={org.id}
                    type='button'
                    onClick={() => navigate(ROUTES.ORG(org.slug).ROOT)}
                    className='flex w-full items-center justify-between rounded-md border bg-background p-3 text-sm transition-colors hover:bg-accent hover:text-accent-foreground'
                  >
                    <span className='font-medium'>{org.name}</span>
                    <span className='text-muted-foreground text-xs capitalize'>{org.role}</span>
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          <div className='space-y-2'>
            {paymentActionUrl ? (
              <Button asChild className='w-full'>
                <a href={paymentActionUrl} target='_blank' rel='noreferrer'>
                  Complete payment
                </a>
              </Button>
            ) : null}

            {showPortalCta && orgId ? (
              <Button
                variant='outline'
                className='w-full'
                onClick={() => portal.mutate()}
                disabled={portal.isPending}
              >
                {portal.isPending ? "Redirecting…" : "Update payment method"}
              </Button>
            ) : null}

            <Button asChild variant='outline' className='w-full'>
              <a href={SUPPORT_MAIL}>Contact account team</a>
            </Button>

            <Button variant='outline' className='w-full' onClick={() => setJoinOpen(true)}>
              Join another organization
            </Button>
          </div>
        </div>
      </div>

      <JoinOrgDialog
        open={joinOpen}
        onOpenChange={setJoinOpen}
        onJoined={(org) => {
          setJoinOpen(false);
          navigate(ROUTES.ORG(org.slug).ROOT);
        }}
      />
    </div>
  );
}
