import { create } from "zustand";
import type { Organization, OrgRole } from "@/types/organization";

interface CurrentOrgState {
  org: Organization | null;
  role: OrgRole | null;
  setOrg: (org: Organization) => void;
  clearOrg: () => void;
}

const useCurrentOrg = create<CurrentOrgState>()((set) => ({
  org: null,
  role: null,
  setOrg: (org) => set({ org, role: org.role }),
  clearOrg: () => set({ org: null, role: null })
}));

export default useCurrentOrg;
