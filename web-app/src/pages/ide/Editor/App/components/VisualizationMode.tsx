import AppPreview from "@/components/AppPreview";

interface VisualizationModeProps {
  modeSwitcher: React.ReactNode;
  appPath: string;
  previewKey: string;
  pathb64: string;
}

export const VisualizationMode = ({
  modeSwitcher,
  appPath,
  previewKey,
  pathb64,
}: VisualizationModeProps) => {
  return (
    <div className="flex flex-col h-full animate-in fade-in duration-200">
      <div className="flex items-center gap-2 px-3 py-1 border-b">
        {modeSwitcher}
        <div className="text-sm font-medium text-muted-foreground">
          {appPath}
        </div>
      </div>
      <div className="flex-1 overflow-hidden">
        <AppPreview key={previewKey} appPath64={pathb64} />
      </div>
    </div>
  );
};
