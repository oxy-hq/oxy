import { type ReactNode, useEffect } from "react";
import { AssumeBanner } from "@/components/admin/AssumeBanner";
import { useActingSession } from "@/hooks/api/adminAssume/useActingSession";

/**
 * Puts the "you are acting as a tenant" banner above **every** authenticated page.
 *
 * It has to live at the root, not inside a layout: the banner used to be mounted
 * in `WorkspaceShell` and `AdminLayout`, but the partner console is neither — so
 * acting as a partner landed you on a page with no banner, hence no exit, while
 * the admin surface was closed. A mode you can enter but not leave is a trap.
 *
 * **It must not add a layout node either.** `.root` is `position: fixed` +
 * `display: flex`, so an in-flow wrapper becomes a stray flex item with no width
 * and collapses the app to content width (this is exactly what a first attempt
 * did). So the banner is a fixed overlay, and `body.is-acting` reserves its height
 * on `.root` in CSS. When no session is live, nothing is rendered and no class is
 * set — the layout is untouched.
 */
export function ActingShell({ children }: { children: ReactNode }) {
  const { isActing } = useActingSession();

  useEffect(() => {
    document.body.classList.toggle("is-acting", isActing);
    return () => document.body.classList.remove("is-acting");
  }, [isActing]);

  return (
    <>
      <AssumeBanner />
      {children}
    </>
  );
}
