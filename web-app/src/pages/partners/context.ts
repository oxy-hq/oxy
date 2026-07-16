import { createContext, useContext } from "react";
import type { MyPartner } from "@/types/partners";

interface PartnerConsoleValue {
  /** Every partner this person holds a role at. */
  partners: MyPartner[];
  /** The one they're currently operating as. */
  active: MyPartner;
  select: (partnerId: string) => void;
}

export const PartnerConsoleContext = createContext<PartnerConsoleValue | null>(null);

/**
 * The active partner, for any page under the console shell.
 *
 * Throws outside the layout rather than returning null: a partner page without a
 * partner is a routing bug, and a silent `undefined` would surface three layers
 * later as an empty table.
 */
export function usePartnerConsole(): PartnerConsoleValue {
  const ctx = useContext(PartnerConsoleContext);
  if (!ctx) throw new Error("usePartnerConsole must be used inside PartnerLayout");
  return ctx;
}
