import { useState } from "react";
import { workspaceLogoUrl } from "@/components/Shell/logoUrl";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

/** Rail-top workspace identity: the org/code-first logo when present, the
 *  name initial otherwise. Pure branding — switching workspaces lives in
 *  the bottom cluster (`RailWorkspaceSwitch`). */
export function RailWorkspaceTile() {
  const { workspace } = useCurrentWorkspace();
  const orgUpdatedAt = useCurrentOrg((s) => s.org?.updated_at);
  const [logoFailed, setLogoFailed] = useState(false);

  const name = workspace?.name ?? "";
  const showLogo = !!workspace?.id && !logoFailed;

  return (
    <span
      data-testid='rail-workspace'
      title={name}
      className='flex h-8 w-8 items-center justify-center overflow-hidden rounded-md bg-primary/10'
    >
      {showLogo && workspace ? (
        <img
          src={workspaceLogoUrl(workspace.id, orgUpdatedAt)}
          alt={name}
          onError={() => setLogoFailed(true)}
          className='h-full w-full object-contain'
        />
      ) : (
        <span className='font-bold text-primary text-xs'>{name.slice(0, 1).toUpperCase()}</span>
      )}
    </span>
  );
}
