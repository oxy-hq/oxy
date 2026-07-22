import { useState } from "react";

/** Rail-top workspace identity: the org/code-first logo when present, the
 *  name initial otherwise. Pure branding — switching workspaces is the host
 *  app's concern (the rail `bottom` slot). */
export function WorkspaceTile({ name, logoUrl }: { name: string; logoUrl?: string }) {
  // Track the URL that failed (not a boolean) so a changed logoUrl —
  // workspace switch, cache-busted re-upload — retries instead of
  // latching the initial fallback for the component's lifetime.
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const showLogo = !!logoUrl && failedUrl !== logoUrl;

  return (
    <span data-testid='rail-workspace' title={name} className='oxy-shell-scope oxy-workspace-tile'>
      {showLogo ? (
        <img
          src={logoUrl}
          alt={name}
          onError={() => setFailedUrl(logoUrl ?? null)}
          className='oxy-workspace-tile__img'
        />
      ) : (
        <span className='oxy-workspace-tile__letter'>{name.slice(0, 1).toUpperCase()}</span>
      )}
    </span>
  );
}
