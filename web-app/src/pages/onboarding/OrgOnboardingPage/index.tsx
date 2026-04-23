import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgs } from "@/hooks/api/organizations";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import OnboardingHeader from "../components/OnboardingHeader";
import InviteStep from "./components/InviteStep";
import WorkspaceStep from "./components/WorkspaceStep";

type Step = "invite" | "workspace";

/**
 * Onboarding wizard for a workspace-less org. Shape depends on the caller:
 *
 *   ?step=invite  → full wizard (invite members → create workspace).
 *                   Only used right after the user creates a brand-new org
 *                   themselves; they're the owner and it makes sense to
 *                   prompt for teammates before building anything.
 *
 *   (no param)    → skip straight to workspace creation. Used when a user
 *                   lands here because their existing org simply has no
 *                   workspace yet (joined via invite, or dispatcher fallback).
 *                   No invite step — they may not even have permission to
 *                   invite, and a freshly-joined member shouldn't start by
 *                   inviting more people.
 *
 * If the org already has ≥1 workspace we bail out to the dispatcher.
 */
export default function OrgOnboardingPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  // Resolve the org from the URL slug, not the zustand store. OrgGuard updates
  // zustand in a useEffect — on the first mount after navigating to a freshly
  // created org's onboarding, zustand still points at the *previous* org, and
  // reading from it here races with the store update. That race would make
  // `useAllWorkspaces` return the old org's cached workspaces (length > 0)
  // and trip the bail-out below, kicking the user back to ROOT before the
  // invite step can render.
  const { orgSlug } = useParams<{ orgSlug: string }>();
  const { data: orgs } = useOrgs();
  const org = orgs?.find((o) => o.slug === orgSlug);
  const orgId = org?.id ?? "";

  const initialStep: Step = searchParams.get("step") === "invite" ? "invite" : "workspace";
  const [step, setStep] = useState<Step>(initialStep);

  const { data: workspaces, isPending } = useAllWorkspaces(orgId);

  // Bail out if the org already has workspaces on mount — that's a dispatcher
  // decision, not an onboarding one. We only check the *initial* fetch so a
  // workspace we create here ourselves (which flips the length 0 → 1) doesn't
  // kick the user out of the preparing screen.
  const mountChecked = useRef(false);
  useEffect(() => {
    if (mountChecked.current) return;
    if (isPending || !workspaces) return;
    mountChecked.current = true;
    if (workspaces.length > 0) {
      navigate(ROUTES.ROOT, { replace: true });
    }
  }, [isPending, workspaces, navigate]);

  if (!org || isPending) {
    return (
      <div className='flex min-h-screen min-w-screen items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  const showInviteStep = initialStep === "invite";

  return (
    <div className='flex min-h-screen w-full flex-col overflow-auto bg-background'>
      <OnboardingHeader />

      <div className='mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-8 px-6 pb-10'>
        {showInviteStep && <StepIndicator current={step} />}

        {step === "invite" ? (
          <InviteStep orgId={orgId} orgName={org.name} onContinue={() => setStep("workspace")} />
        ) : (
          <WorkspaceStep org={org} onBack={showInviteStep ? () => setStep("invite") : undefined} />
        )}
      </div>
    </div>
  );
}

function StepIndicator({ current }: { current: Step }) {
  const steps: { key: Step; label: string }[] = [
    { key: "invite", label: "Invite members" },
    { key: "workspace", label: "Create workspace" }
  ];
  const currentIdx = steps.findIndex((s) => s.key === current);

  return (
    <div className='flex items-center justify-center gap-3 text-sm'>
      {steps.map((s, i) => (
        <div key={s.key} className='flex items-center gap-3'>
          <div
            className={
              i === currentIdx
                ? "flex size-6 items-center justify-center rounded-full bg-primary font-medium text-primary-foreground text-xs"
                : i < currentIdx
                  ? "flex size-6 items-center justify-center rounded-full bg-primary/20 font-medium text-primary text-xs"
                  : "flex size-6 items-center justify-center rounded-full bg-muted font-medium text-muted-foreground text-xs"
            }
          >
            {i + 1}
          </div>
          <span
            className={i === currentIdx ? "font-medium text-foreground" : "text-muted-foreground"}
          >
            {s.label}
          </span>
          {i < steps.length - 1 && <div className='h-px w-6 bg-border' />}
        </div>
      ))}
    </div>
  );
}
