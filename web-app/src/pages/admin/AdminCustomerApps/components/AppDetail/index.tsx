import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/shadcn/sheet";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { CustomerAppsService } from "@/services/api/customerApps";
import type { CustomerApp } from "@/types/apps";
import { Activity } from "./components/Activity";
import { AppInfo } from "./components/AppInfo";
import { AppSettings } from "./components/AppSettings";
import { BuildHistory } from "./components/BuildHistory";
import { type ChannelView, DetailToolbar, type Device } from "./components/DetailToolbar";
import { Functions } from "./components/Functions";
import { LivePreview } from "./components/LivePreview";

/**
 * The stage: a live app preview beside a scrolling "dossier" (status & manifest,
 * builds, activity, settings) — everything about one app on one surface, no
 * sub-tabs.
 *
 * The dossier is collapsible so the preview can go full-bleed, and it adapts to
 * width: on a wide viewport it's a resizable side column; below `lg` it folds
 * into an overlay `Sheet` so the toolbar controls never get squeezed off the
 * row (the resized-Chrome bug this pass fixes). One toolbar button drives both
 * modes.
 *
 * Preview state (device + channel + reload nonce) lives here so the toolbar can
 * drive it and re-selecting a different app keeps the operator's choices.
 */
export const AppDetail = ({ app }: { app: CustomerApp }) => {
  const [device, setDevice] = useState<Device>("desktop");
  // Default to Draft when nothing has been published yet — otherwise the
  // toolbar selects "Published" (disabled) and the iframe requests a
  // published bundle that doesn't exist, hanging the preview. Keep in sync
  // when the parent re-uses this instance for a different app.
  const [channel, setChannel] = useState<ChannelView>(() =>
    app.published_at ? "published" : "draft"
  );
  useEffect(() => {
    setChannel(app.published_at ? "published" : "draft");
  }, [app.published_at]);
  const [channelBusy, setChannelBusy] = useState(false);
  const [nonce, setNonce] = useState(0);

  // Wide = resizable side column; narrow = overlay Sheet. Two bits of state so
  // the pinned side panel and the drawer keep independent defaults (side panel
  // open by default; drawer closed until asked for).
  const isWide = useMediaQuery("(min-width: 1024px)");
  const [dossierPinned, setDossierPinned] = useState(true);
  const [sheetOpen, setSheetOpen] = useState(false);
  const dossierShown = isWide ? dossierPinned : sheetOpen;
  const toggleDossier = () => (isWide ? setDossierPinned((o) => !o) : setSheetOpen((o) => !o));

  // Best-effort cookie cleanup. If staff toggle Draft and then close the
  // admin tab, don't let the preview-draft cookie follow them to a later
  // customer URL view in the same session.
  useEffect(() => {
    return () => {
      void CustomerAppsService.disablePreviewDraft().catch(() => {
        // best-effort
      });
    };
  }, []);

  const handleChannelChange = async (next: ChannelView) => {
    if (next === channel || channelBusy) return;
    setChannelBusy(true);
    try {
      if (next === "draft") {
        await CustomerAppsService.enablePreviewDraft();
      } else {
        await CustomerAppsService.disablePreviewDraft();
      }
      setChannel(next);
      setNonce((n) => n + 1);
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to switch channel";
      toast.error(msg);
    } finally {
      setChannelBusy(false);
    }
  };

  const preview = (
    <div className='flex h-full min-h-0 flex-col'>
      <LivePreview app={app} device={device} channel={channel} nonce={nonce} />
    </div>
  );

  return (
    <div className='flex h-full min-h-0 flex-col bg-background'>
      <DetailToolbar
        app={app}
        tab='preview'
        device={device}
        channel={channel}
        channelBusy={channelBusy}
        showTabs={false}
        dossierOpen={dossierShown}
        onToggleDossier={toggleDossier}
        onTabChange={() => undefined}
        onDeviceChange={setDevice}
        onChannelChange={(c) => void handleChannelChange(c)}
        onReload={() => setNonce((n) => n + 1)}
      />

      <div className='min-h-0 flex-1'>
        {isWide && dossierPinned ? (
          <ResizablePanelGroup direction='horizontal' className='h-full min-h-0'>
            <ResizablePanel defaultSize={58} minSize={32}>
              {preview}
            </ResizablePanel>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={42} minSize={26}>
              <div className='h-full min-h-0 overflow-auto'>
                <DossierColumn app={app} />
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        ) : (
          preview
        )}
      </div>

      {!isWide && (
        <Sheet open={sheetOpen} onOpenChange={setSheetOpen}>
          <SheetContent side='right' className='w-full gap-0 overflow-y-auto p-0 sm:max-w-md'>
            <SheetHeader className='sticky top-0 z-10 border-b bg-background px-4 py-3'>
              <SheetTitle className='text-sm'>Status &amp; details</SheetTitle>
            </SheetHeader>
            <DossierColumn app={app} />
          </SheetContent>
        </Sheet>
      )}
    </div>
  );
};

/** The stacked dossier sections, shared by the wide side column and the narrow
 *  overlay sheet so the two can't drift. */
const DossierColumn = ({ app }: { app: CustomerApp }) => (
  <>
    <DossierSection title='Status & manifest'>
      <AppInfo app={app} />
    </DossierSection>
    <DossierSection title='Build history'>
      <div className='p-4'>
        <BuildHistory appId={app.id} />
      </div>
    </DossierSection>
    <DossierSection title='Functions'>
      <div className='p-4'>
        <Functions appId={app.id} />
      </div>
    </DossierSection>
    <DossierSection title='Activity'>
      <Activity appId={app.id} />
    </DossierSection>
    <DossierSection title='Settings'>
      <AppSettings app={app} />
    </DossierSection>
  </>
);

/**
 * A titled block in the dossier column. A sticky, monospace-eyebrow header
 * keeps the operator oriented while scrolling through the stacked sections.
 */
const DossierSection = ({ title, children }: { title: string; children: React.ReactNode }) => (
  <section className='border-border/60 border-b last:border-b-0'>
    <div className='sticky top-0 z-10 flex items-center border-border/60 border-b bg-background/95 px-4 py-2 backdrop-blur supports-[backdrop-filter]:bg-background/80'>
      <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.16em]'>
        {title}
      </span>
    </div>
    {children}
  </section>
);
