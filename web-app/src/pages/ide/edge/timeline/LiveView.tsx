import { Radio } from "lucide-react";
import type React from "react";
import CameraThumbnail from "../components/CameraThumbnail";

/**
 * "Live" mode for the single-camera timeline — fast-polling snapshot
 * stream that lets the operator see what's happening at the camera
 * right now, like UniFi Protect's live view.
 *
 * Today: drives the existing `CameraThumbnail` (snapshot proxy with
 * configurable polling cadence) at 1s instead of the table-row
 * default of 5s. The proxy normalises everything to JPEG so this
 * works across every camera vendor without any client-side codec
 * concerns. Future: when WebRTC playback is wired end-to-end
 * (Phase 1 plan is mostly there), swap this component to use the
 * WHEP signaling path for sub-500ms latency. The wrapper stays so
 * callers don't have to know which transport is live.
 */
const LiveView: React.FC<{
  cameraId: string;
  cameraName: string | undefined;
}> = ({ cameraId, cameraName }) => (
  <div className='relative h-full w-full bg-black'>
    <CameraThumbnail
      cameraId={cameraId}
      intervalMs={1000}
      className='!h-full !w-full !rounded-none !border-0 !object-contain'
    />
    {/* "LIVE" badge — small, top-left, with a pulsing dot so the
        operator can tell at a glance this is live not a recording. */}
    <div className='absolute top-2 left-2 flex items-center gap-1.5 rounded bg-black/60 px-2 py-1 text-white text-xs'>
      <Radio className='h-3 w-3 animate-pulse text-red-500' />
      <span className='font-semibold tracking-wide'>LIVE</span>
      {cameraName && <span className='text-white/80'>· {cameraName}</span>}
    </div>
  </div>
);

export default LiveView;
