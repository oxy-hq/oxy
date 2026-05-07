import { isAxiosError } from "axios";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { AirhouseLogo } from "@/components/icons";
import useAddAirhouseToConfig from "@/hooks/api/airhouse/useAddAirhouseToConfig";
import useAirhouseConnection from "@/hooks/api/airhouse/useAirhouseConnection";
import useProvisionAirhouse from "@/hooks/api/airhouse/useProvisionAirhouse";
import useRevealAirhouseCredentials from "@/hooks/api/airhouse/useRevealAirhouseCredentials";
import useRotateAirhousePassword from "@/hooks/api/airhouse/useRotateAirhousePassword";
import type { AirhouseCredentials } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import SectionHeader from "../../../components/SectionHeader";
import { ConnectionDetails } from "./components/ConnectionDetails";
import { CredentialsReveal } from "./components/CredentialsReveal";
import { ExampleSnippets } from "./components/ExampleSnippets";
import { ProvisionPrompt } from "./components/ProvisionPrompt";

function statusFromError(err: unknown): number | undefined {
  return isAxiosError(err) ? err.response?.status : undefined;
}

const Airhouse: React.FC = () => {
  const { workspace } = useCurrentWorkspace();
  const workspaceId = workspace?.id;
  const { data: connection, isLoading, error } = useAirhouseConnection(workspaceId);
  const reveal = useRevealAirhouseCredentials(workspaceId);
  const rotate = useRotateAirhousePassword(workspaceId);
  const provision = useProvisionAirhouse(workspaceId);
  const addToConfig = useAddAirhouseToConfig();
  const [shownPassword, setShownPassword] = useState<string | null>(null);
  const [passwordAlreadyRevealed, setPasswordAlreadyRevealed] = useState(false);

  const handleReveal = async () => {
    try {
      const result: AirhouseCredentials = await reveal.mutateAsync();
      if (result.password) {
        setShownPassword(result.password);
        setPasswordAlreadyRevealed(result.password_already_revealed);
      }
    } catch {
      // Error surfaced inline via `reveal.error` in <CredentialsReveal/>.
    }
  };

  const handleRotate = async () => {
    try {
      await rotate.mutateAsync();
      // After rotation the secret has been replaced and `password_revealed_at`
      // is null again. Pull the new password down via the existing reveal hook
      // so we surface it inline without forcing the user to click twice.
      const after: AirhouseCredentials = await reveal.mutateAsync();
      if (after.password) {
        setShownPassword(after.password);
        setPasswordAlreadyRevealed(false);
      }
    } catch {
      // Error surfaced inline via `rotate.error` / `reveal.error`.
    }
  };

  const handleProvision = async (tenantName: string) => {
    try {
      await provision.mutateAsync({ tenantName });
    } catch {
      // Error surfaced inline via `provision.error` in <ProvisionPrompt/>.
    }
  };

  const handleAddToConfig = async (name: string) => {
    try {
      const result = await addToConfig.mutateAsync({ name });
      if (result === "already_present") {
        toast.info("airhouse_managed is already in config.yml");
      } else {
        toast.success(`Added '${name}' database to config.yml — commit to persist the change`);
      }
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Failed to add airhouse_managed to config.yml"
      );
    }
  };

  const renderContent = () => {
    if (isLoading) {
      return <p className='text-muted-foreground text-sm'>Loading…</p>;
    }
    const status = statusFromError(error);
    if (status === 503) {
      return (
        <p className='text-muted-foreground text-sm'>
          Airhouse is not configured for this deployment. Ask an administrator to set the Airhouse
          environment variables and restart the server.
        </p>
      );
    }
    if (!connection || status === 404) {
      return (
        <ProvisionPrompt
          onProvision={handleProvision}
          isPending={provision.isPending}
          error={provision.error}
        />
      );
    }
    if (error) {
      return (
        <p className='text-muted-foreground text-sm'>
          Failed to load Airhouse connection details. Try refreshing the page.
        </p>
      );
    }
    return (
      <>
        <ConnectionDetails
          connection={connection}
          onAddToConfig={handleAddToConfig}
          isAddingToConfig={addToConfig.isPending}
        />
        <CredentialsReveal
          passwordAlreadyRevealed={passwordAlreadyRevealed}
          shownPassword={shownPassword}
          isRevealing={reveal.isPending}
          isRotating={rotate.isPending}
          revealError={reveal.error}
          rotateError={rotate.error}
          onReveal={handleReveal}
          onRotate={handleRotate}
          onDismissPassword={() => setShownPassword(null)}
        />
        <ExampleSnippets connection={connection} password={shownPassword} />
      </>
    );
  };

  return (
    <div className='flex flex-col gap-5'>
      <SectionHeader
        title={
          <span className='flex items-center gap-2'>
            <AirhouseLogo className='h-4 w-4' />
            Airhouse
          </span>
        }
        description='Connect any Postgres-compatible client to your Airhouse-managed database.'
      />
      <div className='space-y-6'>{renderContent()}</div>
    </div>
  );
};

export default Airhouse;
