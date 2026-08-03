import { isAxiosError } from "axios";
import { Loader2, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Separator } from "@/components/ui/shadcn/separator";
import {
  type AccessScope,
  useAppAccess,
  useGrantablePeople,
  useGrantableTeams,
  useSetAppAccess
} from "@/hooks/api/appAccess";
import type { AppVisibility, GranteeRef, GrantRole } from "@/types/appAccess";
import { GrantList } from "./GrantList";
import { GrantPicker } from "./GrantPicker";
import {
  addGrant,
  estimateReach,
  grantedKeys,
  removeGrant,
  setGrantRole,
  toGrantRows
} from "./grants";
import { VisibilityChoice } from "./VisibilityChoice";

interface AppAccessDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  scope: AccessScope;
  appId: string | null;
  appName: string;
}

/**
 * Who can open one custom app.
 *
 * One component for all three consoles — the org's own settings, the Oxy admin
 * panel, and the partner console. They differ only in the `scope` they pass, which
 * decides which endpoints the hooks call.
 *
 * Editing is local until Save: `visibility` and the grant list move together in one
 * PUT, so a half-applied change can't leave an app restricted with nobody on it.
 */
export function AppAccessDialog({
  open,
  onOpenChange,
  scope,
  appId,
  appName
}: AppAccessDialogProps) {
  // All three queries take `appId` only while open, so nothing fetches for a dialog
  // nobody has opened — the grant picker's teams/people lists especially, which
  // would otherwise fire on every render of the surface hosting the dialog.
  const activeAppId = open ? appId : null;
  const { data: access, isFetching, isError } = useAppAccess(scope, activeAppId);
  const { data: teams } = useGrantableTeams(scope, activeAppId);
  const { data: people, isError: peopleFailed } = useGrantablePeople(scope, activeAppId);
  const setAccess = useSetAppAccess(scope);

  const [visibility, setVisibility] = useState<AppVisibility>("org");
  const [grants, setGrants] = useState<GranteeRef[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Seed the editable state ONCE per open, and only from a SETTLED response.
  //
  // The `isFetching` term is load-bearing, not belt-and-braces: without it, a cached
  // entry would seed the form and record the app id, and the fresh response landing
  // a moment later would be skipped by that same guard — quietly re-introducing the
  // stale copy that Save then writes back over someone else's edit. (`gcTime: 0` on
  // the query means there is normally no cache to return, so this is the second lock
  // on the same door.)
  // `useState`, not `useRef`: the render that flips the spinner off has to be
  // *caused* by seeding. With a ref it happened only because `setGrants` passes a
  // freshly-mapped array — a new identity every time, so React never bails out.
  // That made a rendering guarantee load-bearing on something that reads like a
  // formatting detail: memoize that map, or add a deep-equal guard, and the dialog
  // would spin forever with no error. State makes the dependency explicit and costs
  // nothing, since the seeding render already happens.
  const [seededFor, setSeededFor] = useState<string | null>(null);
  useEffect(() => {
    if (!open) {
      if (seededFor !== null) setSeededFor(null);
      return;
    }
    if (!access || isFetching || seededFor === access.app_id) return;
    setSeededFor(access.app_id);
    setVisibility(access.visibility);
    setGrants(access.grants.map((g) => ({ kind: g.kind, id: g.id, role: g.role })));
    setError(null);
  }, [open, access, isFetching, seededFor]);

  /** Grant rows joined back to their display data, in the order the server sorts. */
  const rows = useMemo(
    () => toGrantRows(grants, teams ?? [], people ?? []),
    [grants, teams, people]
  );

  const alreadyGranted = useMemo(() => grantedKeys(grants), [grants]);

  const handleAdd = (kind: "user" | "team", id: string) =>
    setGrants((prev) => addGrant(prev, kind, id));

  const handleRemove = (kind: "user" | "team", id: string) =>
    setGrants((prev) => removeGrant(prev, kind, id));

  const handleRoleChange = (kind: "user" | "team", id: string, role: GrantRole) =>
    setGrants((prev) => setGrantRole(prev, kind, id, role));

  const handleSave = async () => {
    if (!appId) return;
    setError(null);
    try {
      await setAccess.mutateAsync({ appId, body: { visibility, grants } });
      toast.success(
        visibility === "org"
          ? `Everyone in the organization can open ${appName}`
          : `Access to ${appName} updated`
      );
      onOpenChange(false);
    } catch (err) {
      // 400 is the server rejecting a grantee who isn't an org member — worth
      // saying plainly, because the picker should have made it impossible.
      const message = isAxiosError(err)
        ? err.response?.status === 400
          ? "Someone on this list is no longer a member of the organization. Remove them and try again."
          : err.response?.status === 403
            ? "You don't have permission to change who can open this app."
            : (err.response?.data?.message ?? err.message)
        : "Couldn't save access";
      setError(message);
    }
  };

  // The one readiness test the whole dialog uses: has THIS app's server state been
  // copied into the form yet? Both the spinner and Save key off it, so they can't
  // disagree — a Save enabled before seeding would PUT the defaults (`org`, no
  // grants) and wipe the app's real access list.
  const seeded = !!access && seededFor === access.app_id;

  const restricted = visibility === "members";

  // Gate the grant section on the SERVER copy, not on `grants` — the in-progress
  // edit state. Reading the edit state made the section unmount the moment you
  // removed the last role, taking `GrantPicker` with it: no undo (the removal is
  // unsaved), and no way back without switching to "Only people you choose" and
  // returning. It also made the forward direction a detour — the very state this
  // section exists for, an open app with a non-officer as its administrator, was
  // uncreatable on an app that had no grants yet. Keyed on the saved copy, the
  // section stays mounted for the life of the edit.
  const hasSavedGrants = (access?.grants.length ?? 0) > 0;
  const reachCount = useMemo(
    () => (restricted ? estimateReach(grants, teams ?? []) : null),
    [restricted, grants, teams]
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl'>
        <DialogHeader className='space-y-1 border-b px-6 py-4'>
          <DialogTitle className='flex items-center gap-2 text-base'>
            <ShieldCheck className='size-4 text-muted-foreground' aria-hidden />
            Who can open {appName}
          </DialogTitle>
          <DialogDescription>
            Changes apply immediately after you save. People who lose access stop seeing the app on
            their home page.
          </DialogDescription>
        </DialogHeader>

        <div className='flex-1 overflow-y-auto px-6 py-5'>
          {/* Spin until this app's state has actually been SEEDED, not merely until
              the query settles. On the render where `access` first arrives the fetch
              has already finished, so an `isFetching` test is false while component
              state is still the defaults — and a restricted app would paint
              "Everyone in the organization" for one frame before snapping. Gating on
              the seed subsumes both `isPending` and `isFetching` and says what's
              meant. `isError` is checked first: on error `access` never arrives, and
              spinning forever would be worse than the message. */}
          {isError ? (
            <p className='py-12 text-center text-muted-foreground text-sm'>
              Couldn't load access settings. Close this and try again.
            </p>
          ) : !seeded ? (
            <div className='flex items-center justify-center py-16'>
              <Loader2 className='size-5 animate-spin text-muted-foreground' aria-hidden />
              <span className='sr-only'>Loading access settings</span>
            </div>
          ) : (
            <div className='flex flex-col gap-6'>
              <VisibilityChoice value={visibility} onChange={setVisibility} />

              {/* Shown on BOTH branches, not just the restricted one.
                  The server writes the grant list either way — deliberately, because
                  an admin grant on an open app is how a non-officer becomes that
                  app's administrator (`Ring::AppAdmin` has no visibility term). But
                  hiding the list when someone picks "Everyone in the organization"
                  meant the grants silently survived a change the admin reasonably
                  reads as removing them: every member of a team granted `admin` kept
                  the app's privileged surface, with no screen anywhere showing it and
                  no way to remove it without first flipping back to restricted. */}
              {(restricted || hasSavedGrants || grants.length > 0) && (
                <>
                  <Separator />
                  <section className='flex flex-col gap-3'>
                    <div className='flex items-baseline justify-between gap-3'>
                      <h3 className='font-medium text-sm'>
                        {restricted ? "Access list" : "Roles"}
                      </h3>
                      {restricted && reachCount !== null && grants.length > 0 && (
                        <p className='text-muted-foreground text-xs'>
                          Reaches about {reachCount} {reachCount === 1 ? "person" : "people"}
                        </p>
                      )}
                    </div>

                    {!restricted && (
                      <p className='text-muted-foreground text-xs leading-relaxed'>
                        Everyone in the organization can open this app, so these don't control who
                        gets in. <strong>Can administer</strong> still grants the app's admin
                        surface — remove anyone who shouldn't have it. <strong>Can open</strong>{" "}
                        applies again only if you restrict the app.
                      </p>
                    )}

                    <GrantPicker
                      teams={teams ?? []}
                      people={people ?? []}
                      peopleUnavailable={peopleFailed}
                      alreadyGranted={alreadyGranted}
                      onAdd={handleAdd}
                    />

                    {/* Say it out loud rather than showing a teams-only picker and
                        letting individual grants render as "Unknown person" — which
                        would then be saved back under that label. */}
                    {peopleFailed && (
                      <p className='text-muted-foreground text-xs'>
                        Individual people couldn't be loaded, so only teams can be added right now.
                        Any people already on the list are shown below and will be kept as they are.
                      </p>
                    )}

                    <GrantList
                      rows={rows}
                      restricted={restricted}
                      onRoleChange={handleRoleChange}
                      onRemove={handleRemove}
                    />
                  </section>
                </>
              )}

              {error && (
                <p role='alert' className='text-destructive text-sm'>
                  {error}
                </p>
              )}
            </div>
          )}
        </div>

        <div className='flex items-center justify-end gap-2 border-t px-6 py-4'>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!seeded || isError || setAccess.isPending}>
            {setAccess.isPending && <Loader2 className='size-4 animate-spin' aria-hidden />}
            Save access
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
