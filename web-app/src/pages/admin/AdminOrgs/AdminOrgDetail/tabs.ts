/** Tabs on the org-360 surface. `billing` is owner-only (see AdminOrgDetail). */
export type OrgTabId =
  | "overview"
  | "members"
  | "workspaces"
  | "activity"
  | "compiles"
  | "oltp"
  | "billing"
  | "settings";
