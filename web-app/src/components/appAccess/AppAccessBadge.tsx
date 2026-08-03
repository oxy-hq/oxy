import { Building2, Lock } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import type { AppVisibility } from "@/types/appAccess";

/**
 * An app's visibility at a glance, for list rows across the three consoles.
 *
 * Restricted is the state worth noticing, so it gets the outline and the icon;
 * open is the default and stays quiet. Neither uses color alone to carry the
 * meaning — the icon and the word do the work.
 */
export function AppAccessBadge({
  visibility,
  grantCount
}: {
  visibility: AppVisibility;
  /**
   * How many grants the app carries. Shown on BOTH branches: on a restricted app
   * it's the access list, and on an open one it's the surviving roles — which is
   * the more surprising of the two, and the reason this isn't restricted-only.
   */
  grantCount?: number;
}) {
  if (visibility !== "members") {
    // The count shows here too. An open app with grants on it isn't a contradiction
    // — an admin grant is how a non-officer administers an app — but it IS the
    // surprising state, and a bare "Whole org" was the only thing standing between
    // an admin and a role they thought they'd removed when they opened the app up.
    return (
      <Badge variant='secondary' className='gap-1 font-normal'>
        <Building2 className='size-3' aria-hidden />
        Whole org
        {grantCount !== undefined && grantCount > 0 && (
          <span
            className='text-muted-foreground'
            title={`${grantCount} ${grantCount === 1 ? "role" : "roles"} still assigned`}
          >
            · {grantCount}
          </span>
        )}
      </Badge>
    );
  }
  return (
    <Badge variant='outline' className='gap-1 font-normal'>
      <Lock className='size-3' aria-hidden />
      Restricted
      {grantCount !== undefined && grantCount > 0 && (
        <span className='text-muted-foreground'>· {grantCount}</span>
      )}
    </Badge>
  );
}
