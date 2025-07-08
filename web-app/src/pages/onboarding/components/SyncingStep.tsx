interface SyncingStepProps {
  repositorySyncStatus: "idle" | "syncing" | "synced" | "error";
}

export const SyncingStep = ({ repositorySyncStatus }: SyncingStepProps) => {
  return (
    <div className="border rounded-lg p-6 bg-card">
      <div className="space-y-4">
        {repositorySyncStatus === "idle" && (
          <div className="flex items-center">
            <div>
              <h3 className="text-lg font-medium">Repository Ready</h3>
              <p className="text-sm text-muted-foreground">
                Your repository is ready to be cloned.
              </p>
            </div>
          </div>
        )}
        {repositorySyncStatus === "syncing" && (
          <div className="flex items-center">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary mr-3"></div>
            <div>
              <h3 className="text-lg font-medium">Cloning Repository</h3>
              <p className="text-sm text-muted-foreground">
                Please wait while we clone your repository. This may take a few
                minutes.
              </p>
            </div>
          </div>
        )}
        {repositorySyncStatus === "synced" && (
          <div className="flex items-center">
            <div>
              <p className="text-sm text-muted-foreground">
                Your repository has been successfully cloned and is ready to
                use.
              </p>
            </div>
          </div>
        )}
        {repositorySyncStatus === "error" && (
          <div className="flex items-center">
            <div>
              <h3 className="text-lg font-medium text-red-600">Sync Error</h3>
              <p className="text-sm text-muted-foreground">
                There was an error cloning your repository. Please try again.
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
