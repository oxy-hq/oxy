import { useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import WorkspaceCreator, {
  type WorkspaceCreationPhase
} from "@/components/workspaces/components/WorkspaceCreator";

/**
 * Workspace-creation step of the org onboarding wizard. Wraps
 * <WorkspaceCreator /> in a card and tracks the creator's internal phase so
 * we can swap the card header between "create" and "preparing".
 */
export default function WorkspaceStep({
  org,
  onBack
}: {
  /** Passed explicitly because OrgGuard skips persisting the org to zustand
   *  while on the onboarding route (otherwise the empty org would trap the
   *  dispatcher in a loop). WorkspaceCreator's zustand fallback would read
   *  the *previous* org and create the workspace in the wrong place. */
  org: { id: string; name: string; slug: string };
  /** Omit when there is no prior step to return to (e.g. the user arrived
   *  here via the has-org-no-workspace fallback, not the invite wizard). */
  onBack?: () => void;
}) {
  const [phase, setPhase] = useState<WorkspaceCreationPhase>("create");
  const preparing = phase === "preparing";

  return (
    <Card>
      <CardHeader>
        <CardTitle className='text-lg'>
          {preparing ? "Preparing your workspace" : "Create your first workspace"}
        </CardTitle>
        <CardDescription>
          {preparing
            ? "Hang tight — we're getting everything ready."
            : `Spin up a workspace for ${org.name}. Import from GitHub, start with a demo, or go blank.`}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <WorkspaceCreator
          org={{ id: org.id, slug: org.slug }}
          onBack={onBack}
          onPhaseChange={setPhase}
        />
      </CardContent>
    </Card>
  );
}
