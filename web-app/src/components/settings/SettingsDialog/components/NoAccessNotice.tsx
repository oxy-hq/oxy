import type { ReactNode } from "react";

/**
 * The denial state for a settings section the caller cannot administer.
 *
 * Sections the caller can't act on are already hidden from the nav (see the
 * `requires` gates in `SettingsDialog/nav.ts`), so this is the second line of
 * defence — a deep link, a store-restored `activeSection`, or a role that
 * changes while the dialog is open must land on an explanation rather than an
 * empty pane. Rendering nothing was the original bug: an org Member opening
 * General got a blank panel with no way to tell it apart from a load failure.
 */
export default function NoAccessNotice({ children }: { children: ReactNode }) {
  return (
    <div className='flex items-center justify-center py-12'>
      <p className='text-muted-foreground text-sm'>{children}</p>
    </div>
  );
}
