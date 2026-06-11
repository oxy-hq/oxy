import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { CustomerAppsService } from "@/services/api/customerApps";
import type { CustomerApp } from "@/types/apps";
import { Activity } from "./components/Activity";
import { AppInfo } from "./components/AppInfo";
import { AppSettings } from "./components/AppSettings";
import { BuildHistory } from "./components/BuildHistory";
import { type ChannelView, DetailToolbar, type Device } from "./components/DetailToolbar";
import { LivePreview } from "./components/LivePreview";

/**
 * Right pane — one empowered surface, no sub-tabs. The 2026-06 pass collapses
 * the old Preview / Info / Activity / Settings tab strip into a single view:
 * the live preview sits on the left and a scrolling "dossier" on the right
 * shows every section at once (status & manifest, build history, activity,
 * settings). Nothing useful is a tab-click away any more.
 *
 * Preview state (device + channel + reload nonce) lives here so the toolbar
 * can drive it and re-selecting a different app doesn't lose the staff
 * member's choices.
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

  return (
    <div className='flex h-full min-h-0 flex-col bg-background'>
      <DetailToolbar
        app={app}
        tab='preview'
        device={device}
        channel={channel}
        channelBusy={channelBusy}
        showTabs={false}
        onTabChange={() => undefined}
        onDeviceChange={setDevice}
        onChannelChange={(c) => void handleChannelChange(c)}
        onReload={() => setNonce((n) => n + 1)}
      />

      <ResizablePanelGroup direction='horizontal' className='min-h-0 flex-1'>
        <ResizablePanel defaultSize={58} minSize={32}>
          <div className='flex h-full min-h-0 flex-col'>
            <LivePreview app={app} device={device} channel={channel} nonce={nonce} />
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={42} minSize={26}>
          <div className='h-full min-h-0 overflow-auto'>
            <DossierSection title='Status & manifest'>
              <AppInfo app={app} />
            </DossierSection>
            <DossierSection title='Build history'>
              <div className='p-4'>
                <BuildHistory appId={app.id} />
              </div>
            </DossierSection>
            <DossierSection title='Activity'>
              <Activity appId={app.id} />
            </DossierSection>
            <DossierSection title='Settings'>
              <AppSettings app={app} />
            </DossierSection>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
};

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
