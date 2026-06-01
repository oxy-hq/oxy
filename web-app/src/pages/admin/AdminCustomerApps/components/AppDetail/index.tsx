import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { CustomerAppsService } from "@/services/api/customerApps";
import type { CustomerApp } from "@/types/apps";
import { AppInfo } from "./components/AppInfo";
import { AppSettings } from "./components/AppSettings";
import { BuildHistory } from "./components/BuildHistory";
import {
  type ChannelView,
  type DetailTab,
  DetailToolbar,
  type Device
} from "./components/DetailToolbar";
import { LivePreview } from "./components/LivePreview";

const TABS: DetailTab[] = ["preview", "info", "settings"];

/**
 * Right pane: unified command bar + active section. The previous
 * shape stacked three toolbars (header → tab strip → preview chrome)
 * which competed for vertical space; this consolidates them into a
 * single `DetailToolbar` that re-renders its contextual half per
 * active section.
 *
 * Preview-related state (device + channel + reload nonce) lives here
 * rather than inside `LivePreview` so the toolbar can drive it from
 * one row up, and a tab switch doesn't blow away the staff member's
 * device / channel choices.
 */
export const AppDetail = ({ app }: { app: CustomerApp }) => {
  const [params, setParams] = useSearchParams();
  const raw = params.get("tab") ?? "preview";
  const tab: DetailTab = TABS.includes(raw as DetailTab) ? (raw as DetailTab) : "preview";

  const [device, setDevice] = useState<Device>("desktop");
  const [channel, setChannel] = useState<ChannelView>("published");
  const [channelBusy, setChannelBusy] = useState(false);
  const [nonce, setNonce] = useState(0);

  const setTab = (next: DetailTab) => {
    const params2 = new URLSearchParams(params);
    params2.set("tab", next);
    setParams(params2, { replace: true });
  };

  // Best-effort cookie cleanup. If staff toggle Draft and then close
  // the admin tab, we don't want the preview-draft cookie following
  // them to a subsequent customer URL view in the same session.
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
      // Bump the nonce so the iframe re-navigates with the updated
      // cookie. Cheap; the URL is unchanged so the bundle's runtime
      // config is identical.
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
        tab={tab}
        device={device}
        channel={channel}
        channelBusy={channelBusy}
        onTabChange={setTab}
        onDeviceChange={setDevice}
        onChannelChange={(c) => void handleChannelChange(c)}
        onReload={() => setNonce((n) => n + 1)}
      />

      {/* Section content. Preview stays mounted across tab switches
          (display:none via `hidden`) so flipping back doesn't remount
          the iframe; Info + Settings unmount because their state is
          cheap and the data is paged. */}
      <div
        className={`flex min-h-0 flex-1 flex-col ${tab === "preview" ? "" : "hidden"}`}
        aria-hidden={tab !== "preview"}
      >
        <LivePreview app={app} device={device} channel={channel} nonce={nonce} />
      </div>
      {tab === "info" && (
        <div className='min-h-0 flex-1 overflow-auto'>
          <AppInfo app={app} />
          <div className='p-4'>
            <BuildHistory appId={app.id} />
          </div>
        </div>
      )}
      {tab === "settings" && (
        <div className='min-h-0 flex-1 overflow-auto'>
          <AppSettings app={app} />
        </div>
      )}
    </div>
  );
};
