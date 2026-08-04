import { AppWindow } from "lucide-react";
import type React from "react";
import { useMemo, useState } from "react";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { useAppIntegrations } from "@/hooks/api/appIntegrations/useAppIntegrations";
import type { AppIntegration, AppIntegrationKind } from "@/services/api/appIntegrations";
import NoAccessNotice from "../../../components/NoAccessNotice";
import SectionHeader from "../../../components/SectionHeader";
import { AppCard } from "./components/AppCard";
import { ConfigureAppDialog } from "./components/ConfigureAppDialog";

const APP_KINDS: AppIntegrationKind[] = ["toast", "openweathermap", "besttime", "unifi"];

const Apps: React.FC = () => {
  const { data, isLoading } = useAppIntegrations();
  const [activeKind, setActiveKind] = useState<AppIntegrationKind | null>(null);

  const byKind = useMemo(() => {
    const map = new Map<AppIntegrationKind, AppIntegration>();
    for (const entry of data ?? []) map.set(entry.kind, entry);
    return map;
  }, [data]);

  return (
    <CanWorkspaceAdmin
      fallback={<NoAccessNotice>You need workspace admin access to manage apps.</NoAccessNotice>}
    >
      <div className='flex flex-col gap-5'>
        <SectionHeader
          icon={AppWindow}
          title='Apps'
          description='External services that feed downstream dashboards. Credentials live in the workspace secrets store and are referenced from config.yml.'
        />

        <div className='grid grid-cols-1 gap-3 md:grid-cols-2'>
          {APP_KINDS.map((kind) => (
            <AppCard
              key={kind}
              kind={kind}
              integration={byKind.get(kind) ?? null}
              loading={isLoading}
              onConfigure={() => setActiveKind(kind)}
            />
          ))}
        </div>

        {activeKind && (
          <ConfigureAppDialog
            kind={activeKind}
            existing={byKind.get(activeKind) ?? null}
            onClose={() => setActiveKind(null)}
          />
        )}
      </div>
    </CanWorkspaceAdmin>
  );
};

export default Apps;
