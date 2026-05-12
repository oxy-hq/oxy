import type { SettingsSection } from "@/stores/useSettingsDialog";
import type { Organization, OrgRole } from "@/types/organization";
import type { Workspace } from "@/types/workspace";
import Billing from "../sections/organization/Billing";
import General from "../sections/organization/General";
import Integration from "../sections/organization/Integration";
import OrgMembers from "../sections/organization/Members";
import ActivityLogs from "../sections/workspace/ActivityLogs";
import Airhouse from "../sections/workspace/Airhouse";
import ApiKeys from "../sections/workspace/ApiKeys";
import Databases from "../sections/workspace/Databases";
import WorkspaceMembers from "../sections/workspace/Members";
import Repositories from "../sections/workspace/Repositories";
import Secrets from "../sections/workspace/Secrets";

export type ActiveSectionProps = {
  activeSection: SettingsSection;
  org: Organization | null;
  role: OrgRole | null;
  isAdmin: boolean;
  workspace: Workspace | null;
  isLocalMode: boolean;
  close: () => void;
};

/**
 * Renders whichever settings section matches `activeSection`. Shared between
 * the mobile and desktop layouts so section selection logic lives in one place.
 */
export function ActiveSection({
  activeSection,
  org,
  role,
  isAdmin,
  workspace,
  isLocalMode,
  close
}: ActiveSectionProps) {
  return (
    <>
      {org && role && activeSection === "organization.general" && (
        <General org={org} onClose={close} />
      )}
      {org && role && activeSection === "organization.members" && (
        <OrgMembers org={org} viewerRole={role} />
      )}
      {org && role && activeSection === "organization.billing" && isAdmin && (
        <Billing org={org} onClose={close} />
      )}
      {org && activeSection === "organization.integration" && <Integration org={org} />}

      {workspace && activeSection === "workspace.members" && <WorkspaceMembers />}
      {workspace && activeSection === "workspace.databases" && <Databases />}
      {workspace && activeSection === "workspace.airhouse" && <Airhouse />}
      {workspace && activeSection === "workspace.repositories" && <Repositories />}
      {workspace && activeSection === "workspace.api_keys" && <ApiKeys />}
      {workspace && activeSection === "workspace.secrets" && (isLocalMode || isAdmin) && (
        <Secrets />
      )}
      {workspace && activeSection === "workspace.activity_logs" && <ActivityLogs />}
    </>
  );
}

export default ActiveSection;
