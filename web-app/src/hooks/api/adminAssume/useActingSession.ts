import { useCurrentAssume } from "@/hooks/api/adminAssume";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import type { AssumeSession } from "@/types/adminAssume";

/**
 * Where an assumed session belongs.
 *
 * Acting as a **partner** lands you in the partner console; acting as a plain org
 * lands you in that org's product. Landing anywhere else — in particular, staying
 * in the admin panel — makes the mode pointless: you would be "acting as" someone
 * while looking at a screen they can't see.
 */
export function landingFor(session: AssumeSession): string {
  if (session.is_partner) return "/partners";
  return session.org_slug ? `/${session.org_slug}` : "/";
}

/**
 * The single answer to "am I currently acting as a tenant?".
 *
 * The server is the authority — it refuses the whole staff surface while a session
 * is live (`assume::block_admin_while_acting`) and synthesizes the tenant's reach
 * on the way in. This hook only keeps the UI from showing a door the server would
 * slam anyway.
 */
export function useActingSession(): {
  session: AssumeSession | undefined;
  isActing: boolean;
  landing: string | null;
  /** Where "stop acting" should return them: their own console. */
  home: string;
} {
  const { data: user } = useCurrentUser();
  const isStaff = !!(user?.is_owner || user?.is_app_admin);
  const isPartner = (user?.partner_memberships?.length ?? 0) > 0;
  // Staff act as any org; a partner acts as an assigned client. Nobody else can
  // hold a session, so don't poll for them.
  const { data: sessions } = useCurrentAssume(isStaff || isPartner);

  const session = isStaff || isPartner ? sessions?.[0] : undefined;
  return {
    session,
    isActing: !!session,
    landing: session ? landingFor(session) : null,
    // Staff came from admin; a partner came from their console. Returning someone
    // to a surface they can't reach would just 403 them at the door.
    home: isStaff ? "/admin/tenants" : "/partners"
  };
}
