import { useState } from "react";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../../../../resolveBundleUrl";
import type { ChannelView, Device } from "../DetailToolbar";
import { DebugPanel } from "./DebugPanel";
import { useOxyRequestLog } from "./useOxyRequestLog";

/**
 * Iframe canvas for the customer-app preview. All chrome (device
 * selector, channel toggle, reload, URL) lives in the unified
 * `DetailToolbar` one row up — this component owns only the canvas
 * + device frame, so the visible vertical real estate goes to the
 * actual customer app.
 *
 * Two visual modes:
 *   - **Desktop**: full-bleed. iframe fills the pane edge-to-edge,
 *     no border / shadow / padding. You're looking AT the customer
 *     app, not at "an iframe of one". This is the immersive default
 *     because most customer apps are rendered for desktop.
 *   - **Mobile / Tablet**: device-shaped card on a dotted backdrop
 *     with a soft accent halo underneath. Aspect ratios are real
 *     (iPhone-ish 9:19.5, iPad-ish 3:4) so a long page scrolls inside
 *     the frame instead of stretching to fit. A mono dimensions
 *     label sits below the device for context.
 *
 * Controlled by `AppDetail` — device / channel / nonce are props so
 * the toolbar one row up can drive them without remounting the iframe.
 */
export interface LivePreviewProps {
  app: CustomerApp;
  device: Device;
  /** Used purely to force a fresh iframe navigation; bumped when the
   *  caller wants to refetch (reload, channel toggle). */
  nonce: number;
  /** Mirrors the toolbar's selected channel — only used as part of
   *  the iframe `key` so a channel flip on the server (cookie toggle)
   *  always lands a re-navigation, even if the nonce hasn't moved. */
  channel: ChannelView;
}

/**
 * Device specs. Widths follow common v0 / Vercel preview conventions;
 * the aspect ratios are the real device shapes so a long customer app
 * scrolls inside the device frame instead of stretching.
 */
const DEVICE_SPECS: Record<
  Device,
  { width: number | "100%"; aspect: string; label: string; cornerRadius: string }
> = {
  mobile: {
    width: 390,
    aspect: "9 / 19.5",
    label: "390 × 844",
    cornerRadius: "2rem"
  },
  tablet: {
    width: 820,
    aspect: "3 / 4",
    label: "820 × 1180",
    cornerRadius: "1.25rem"
  },
  desktop: {
    width: "100%",
    aspect: "auto",
    label: "",
    cornerRadius: "0"
  }
};

export const LivePreview = ({ app, device, nonce, channel }: LivePreviewProps) => {
  // resolveBundleUrl rewrites the host to the admin shell's own origin
  // so cookies travel through Vite's /customer-apps proxy in dev (and
  // is a no-op in production where admin + bundles share an origin).
  // The nonce query param cache-busts the iframe — bumped on Reload,
  // on channel flip, and on first mount — without changing the path.
  const previewUrl = (() => {
    const u = new URL(resolveBundleUrl(app.url));
    u.searchParams.set("_oxy_preview", String(nonce));
    return u.toString();
  })();

  // Same-origin preview → instrument the iframe to capture its calls to oxy.
  const { entries, clear, handleLoad, available } = useOxyRequestLog();
  const [panelOpen, setPanelOpen] = useState(false);
  const onIframeLoad = (el: HTMLIFrameElement) => handleLoad(el);

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex min-h-0 flex-1 flex-col'>
        {device === "desktop" ? (
          <DesktopFrame
            app={app}
            url={previewUrl}
            channel={channel}
            nonce={nonce}
            onIframeLoad={onIframeLoad}
          />
        ) : (
          <DeviceFrame
            app={app}
            url={previewUrl}
            device={device}
            channel={channel}
            nonce={nonce}
            onIframeLoad={onIframeLoad}
          />
        )}
      </div>
      <DebugPanel
        entries={entries}
        onClear={clear}
        open={panelOpen}
        onToggle={() => setPanelOpen((o) => !o)}
        available={available}
      />
    </div>
  );
};

/**
 * Full-bleed desktop. No card, no border, no padding — just the
 * iframe filling the pane. The toolbar's bottom border is the only
 * separator. This is the immersive case: you forget you're in an
 * admin shell.
 */
const DesktopFrame = ({
  app,
  url,
  channel,
  nonce,
  onIframeLoad
}: {
  app: CustomerApp;
  url: string;
  channel: ChannelView;
  nonce: number;
  onIframeLoad: (el: HTMLIFrameElement) => void;
}) => (
  <div className='min-h-0 flex-1 bg-background'>
    <iframe
      onLoad={(e) => onIframeLoad(e.currentTarget)}
      // No `sandbox` attribute: this preview always loads from the
      // same origin we ship the customer-app code from, and the app
      // needs cookies + localStorage + same-origin fetch to function.
      // The only sandbox tokens that'd let those through
      // (`allow-same-origin` + `allow-scripts`) are documented by the
      // HTML spec as "effectively no sandbox" — combining them lets
      // the framed page reach into its parent and remove its own
      // sandbox. So the attribute was security theater. Trust model:
      // we ship the bundle code; origin gating is what actually
      // protects the admin shell.
      key={`desktop-${channel}-${nonce}`}
      src={url}
      title={`${app.name} live preview`}
      className='size-full border-0'
    />
  </div>
);

/**
 * Mobile / tablet device frame. Centered on a dotted backdrop with a
 * soft accent halo beneath the device so it reads as a physical
 * object floating on a stage, not a constrained div.
 */
const DeviceFrame = ({
  app,
  url,
  device,
  channel,
  nonce,
  onIframeLoad
}: {
  app: CustomerApp;
  url: string;
  device: Exclude<Device, "desktop">;
  channel: ChannelView;
  nonce: number;
  onIframeLoad: (el: HTMLIFrameElement) => void;
}) => {
  const spec = DEVICE_SPECS[device];

  return (
    <div
      className='relative flex min-h-0 flex-1 items-center justify-center overflow-auto bg-muted/30 p-8'
      style={{
        // Dotted grid backdrop — Vercel-preview-style. Inline so the
        // dot color tracks the theme's border token; --border resolves
        // to oklch in both light and dark.
        backgroundImage: "radial-gradient(circle, var(--border) 1px, transparent 1px)",
        backgroundSize: "22px 22px"
      }}
    >
      {/* Soft accent halo beneath the device. Sits behind everything
          (negative z) with heavy blur — gives the device a faint glow
          ground so it doesn't read as floating in a vacuum. Color
          ties to the brand primary so the eye reads it as intentional
          rather than incidental. */}
      <div
        aria-hidden
        className='pointer-events-none absolute top-1/2 left-1/2 size-[60%] -translate-x-1/2 rounded-full bg-primary/15 opacity-60 blur-3xl'
      />

      <div
        className='relative flex max-h-full flex-col items-center'
        style={{ width: typeof spec.width === "number" ? `${spec.width}px` : spec.width }}
      >
        {/* Device card. Aspect-ratio + max-height keep the device
            real-shape while shrinking to fit short panes. The inner
            ring (`ring-1 ring-foreground/5`) reads as a thin bezel
            without going full-skeuomorphic phone art. */}
        <div
          className={cn(
            "relative w-full overflow-hidden bg-background shadow-2xl ring-1 ring-foreground/5 transition-[max-width,border-radius] duration-300",
            "shadow-foreground/20"
          )}
          style={{
            aspectRatio: spec.aspect,
            maxHeight: "calc(100% - 2.5rem)",
            borderRadius: spec.cornerRadius
          }}
        >
          <iframe
            // See DesktopFrame's comment — same reasoning: drop the
            // sandbox attribute rather than combine the two tokens
            // that neutralise each other.
            onLoad={(e) => onIframeLoad(e.currentTarget)}
            key={`${device}-${channel}-${nonce}`}
            src={url}
            title={`${app.name} live preview`}
            className='size-full border-0'
          />
        </div>

        {/* Quiet dimensions label. Mono + uppercase, low opacity,
            sits just below the device — reads as a caption, not a
            control. Confirms the frame is a literal device size. */}
        <span className='mt-3 font-mono text-muted-foreground/70 text-xs uppercase tracking-[0.15em]'>
          {spec.label}
        </span>
      </div>
    </div>
  );
};
