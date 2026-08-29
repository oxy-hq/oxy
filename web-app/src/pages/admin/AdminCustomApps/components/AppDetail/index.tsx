import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/shadcn/sheet";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { CustomAppsService } from "@/services/api/customApps";
import type { CustomApp } from "@/types/apps";
import { type ChannelView, DetailToolbar, type Device } from "./components/DetailToolbar";
import { DockControls, DossierBody, DossierHeader } from "./components/Dossier";
import { LivePreview } from "./components/LivePreview";
import { DOCK_STORAGE_KEY, type DockMode, dossierWindowPath, reviveDockMode } from "./dock";
import { useAppViewState } from "./useAppViewState";
import { useDossierWindow } from "./useDossierWindow";
import { usePersistentState } from "./usePersistentState";

/**
 * The stage: a live app preview beside a scrolling "dossier" (status & manifest,
 * builds, functions, activity, settings) — everything about one app on one
 * surface, no sub-tabs.
 *
 * The dossier is dockable, DevTools-style, because a fixed side column spends
 * the operator's scarcest resource (horizontal space on a laptop) on the
 * content least able to use it — manifest JSON, bundle paths, build rows. So:
 * dock right, dock bottom for the full stage width, or pop out into a real
 * second window. The choice persists per operator.
 *
 * Below `lg` none of that applies — the dossier folds into an overlay `Sheet`
 * so the toolbar controls never get squeezed off the row.
 *
 * ## Where the state lives
 *
 * Device, channel and the preview's own location are **query params**, because
 * they name a place an operator sends a colleague: "Bookkeeping, draft channel,
 * on mobile, showing the vendor screen". Back and Forward then walk those
 * choices, and a reload lands on the same view.
 *
 * Dock mode and whether the dossier is pinned stay in `localStorage`. They are
 * how one operator likes to sit, not a place — a shared link that rearranged
 * the recipient's panels would be a bug rather than a feature. `appViewState.ts`
 * states that split once.
 *
 * The reload nonce stays local: it is an instruction, not a location. Encoding
 * it would mean a shared link forces a refetch, and Back would "un-reload".
 */
export const AppDetail = ({ app }: { app: CustomApp }) => {
  // Default to Draft when nothing has been published yet — otherwise the
  // toolbar selects "Published" (disabled) and the iframe requests a published
  // bundle that doesn't exist, hanging the preview. The default is passed in
  // rather than baked into the reader, so "no ?channel" means the right thing
  // per app instead of the same thing for every app.
  const channelDefault: ChannelView = app.published_at ? "published" : "draft";
  const { view, patch: patchView } = useAppViewState(channelDefault);
  const { device, channel } = view;

  const setDevice = useCallback((next: Device) => patchView({ device: next }), [patchView]);
  const onPreviewPathChange = useCallback(
    (path: string | null) => patchView({ preview: path }, "replace"),
    [patchView]
  );
  const onFnChange = useCallback((name: string | null) => patchView({ fn: name }), [patchView]);

  const [channelBusy, setChannelBusy] = useState(false);
  const [nonce, setNonce] = useState(0);

  // Wide = docked panel; narrow = overlay Sheet. Two bits of state so the
  // docked panel and the drawer keep independent defaults (panel open by
  // default; drawer closed until asked for).
  const isWide = useMediaQuery("(min-width: 1024px)");
  const [dossierPinned, setDossierPinned] = useState(true);
  const [sheetOpen, setSheetOpen] = useState(false);
  const dossierShown = isWide ? dossierPinned : sheetOpen;
  const toggleDossier = () => (isWide ? setDossierPinned((o) => !o) : setSheetOpen((o) => !o));

  const [dock, setDock] = usePersistentState<DockMode>(DOCK_STORAGE_KEY, "right", reviveDockMode);
  // A persisted `window` must NOT auto-open on load: a popup with no user
  // gesture is blocked, which would both toast an error and clobber the saved
  // preference. So window mode only actually pops out once the operator picks it
  // (a real gesture, tracked here); a persisted `window` renders inline as a
  // right dock until then, with the stored choice left intact so one click on
  // the control re-opens it.
  const [windowActivated, setWindowActivated] = useState(false);
  const effectiveDock: DockMode = dock === "window" && !windowActivated ? "right" : dock;
  const poppedOut = isWide && dossierPinned && dock === "window" && windowActivated;
  const handleDockChange = useCallback(
    (next: DockMode) => {
      // Selecting `window` from the control IS the gesture that lets the popup
      // open; record it so the open effect is allowed to run this time.
      if (next === "window") setWindowActivated(true);
      setDock(next);
    },
    [setDock]
  );
  // Closing the popped-out window (or having a user-initiated open blocked) must
  // land somewhere visible, not on an invisible dossier the operator can't get
  // back — and reset the gesture so it doesn't try to reopen on its own.
  const fallBackToSideColumn = useCallback(() => {
    setWindowActivated(false);
    setDock("right");
  }, [setDock]);
  const focusDossierWindow = useDossierWindow({
    active: poppedOut,
    url: dossierWindowPath(app.org_slug, app.slug),
    name: "oxy-app-dossier",
    onDismiss: fallBackToSideColumn
  });

  // Best-effort cookie cleanup. If staff toggle Draft and then close the
  // admin tab, don't let the preview-draft cookie follow them to a later
  // customer URL view in the same session.
  useEffect(() => {
    return () => {
      void CustomAppsService.disablePreviewDraft().catch(() => {
        // best-effort
      });
    };
  }, []);

  const handleChannelChange = async (next: ChannelView) => {
    if (next === channel || channelBusy) return;
    setChannelBusy(true);
    try {
      if (next === "draft") {
        await CustomAppsService.enablePreviewDraft();
      } else {
        await CustomAppsService.disablePreviewDraft();
      }
      // The channel is a server-side cookie, so the URL records the operator's
      // choice but does not *cause* it — a Back that returns to `?channel=draft`
      // re-selects Draft in the toolbar without re-issuing the toggle. Worth
      // knowing: the cookie is per-session, so a link opened by a colleague
      // shows their channel until they flip it themselves.
      patchView({ channel: next, preview: null });
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
      <LivePreview
        app={app}
        device={device}
        channel={channel}
        nonce={nonce}
        path={view.preview}
        onPathChange={onPreviewPathChange}
      />
    </div>
  );

  const dossier = (
    <div className='flex h-full min-h-0 flex-col'>
      <DossierHeader
        dock={effectiveDock}
        onDockChange={handleDockChange}
        onClose={() => setDossierPinned(false)}
      />
      <DossierBody app={app} focusSection={view.section} fn={view.fn} onFnChange={onFnChange} />
    </div>
  );

  const isDockedPanel = isWide && dossierPinned && effectiveDock !== "window";
  const bottom = effectiveDock === "bottom";

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
        {isDockedPanel ? (
          // Keyed by direction: react-resizable-panels sizes against a fixed
          // axis, so flipping horizontal↔vertical needs a fresh group.
          <ResizablePanelGroup
            key={effectiveDock}
            direction={bottom ? "vertical" : "horizontal"}
            className='h-full min-h-0'
          >
            <ResizablePanel defaultSize={bottom ? 55 : 58} minSize={bottom ? 20 : 32}>
              {preview}
            </ResizablePanel>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={bottom ? 45 : 42} minSize={bottom ? 20 : 26}>
              {dossier}
            </ResizablePanel>
          </ResizablePanelGroup>
        ) : (
          preview
        )}
      </div>

      {/* Popped out: the preview owns the whole stage, and this strip keeps the
          dock switcher reachable — otherwise the only way back to a docked
          panel would be to close the window we just opened. */}
      {poppedOut && (
        <div className='flex h-9 shrink-0 items-center gap-2 border-t bg-background px-2'>
          <span className='ml-1 min-w-0 flex-1 truncate text-muted-foreground text-xs'>
            Details are open in a separate window.
          </span>
          <Button variant='ghost' size='sm' className='h-7' onClick={() => focusDossierWindow()}>
            Focus window
          </Button>
          <DockControls value={effectiveDock} onChange={handleDockChange} />
        </div>
      )}

      {!isWide && (
        <Sheet open={sheetOpen} onOpenChange={setSheetOpen}>
          <SheetContent side='right' className='flex w-full flex-col gap-0 p-0 sm:max-w-md'>
            <SheetHeader className='shrink-0 border-b px-4 py-3'>
              <SheetTitle className='text-xs'>Status &amp; details</SheetTitle>
            </SheetHeader>
            <DossierBody
              app={app}
              focusSection={view.section}
              fn={view.fn}
              onFnChange={onFnChange}
            />
          </SheetContent>
        </Sheet>
      )}
    </div>
  );
};
