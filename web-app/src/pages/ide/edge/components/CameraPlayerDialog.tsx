import Hls from "hls.js";
import { Bolt, VideoOff } from "lucide-react";
import type React from "react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type CameraSummary } from "@/services/api";
import { apiBaseURL } from "@/services/env";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

type Props = {
  camera: CameraSummary | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

type Transport = "webrtc" | "hls";
type Status = "loading" | "playing" | "unavailable";

/**
 * How long we wait for the first media frame after WebRTC
 * negotiation completes before giving up and falling back to HLS.
 * 5s is generous — a healthy session has frames within ~500ms.
 */
const WEBRTC_FIRST_FRAME_TIMEOUT_MS = 5000;

/**
 * Safari WebRTC is a known second-class citizen: the
 * H.264-baseline-only constraint trips up most off-the-shelf
 * camera streams. Skip the WebRTC attempt entirely on Safari /
 * iOS to avoid the 5s timeout-then-fallback dance.
 */
function isSafari(): boolean {
  const ua = navigator.userAgent;
  return /^((?!chrome|android).)*safari/i.test(ua);
}

/**
 * Live preview modal. Tries WebRTC first for sub-second latency;
 * falls back to HLS on Safari, on signaling failure, or if no
 * media arrives within the timeout above.
 *
 * Two transport paths share the same `<video>` element:
 *   - WebRTC: browser ⇆ Tailscale Funnel ⇆ MediaMTX on the box.
 *     Oxy is in signaling only. ~200–500ms latency.
 *   - HLS: browser ⇆ Oxy proxy ⇆ MediaMTX on the box. Every byte
 *     through Oxy. ~5–10s latency.
 *
 * Lifecycle: init when `open && camera` → tear down WebRTC pc /
 * Hls instance on close or camera change. Keeping either across
 * closes leaks buffers and keeps a media stream open against
 * the edge.
 */
const CameraPlayerDialog: React.FC<Props> = ({ camera, open, onOpenChange }) => {
  const { workspace } = useCurrentWorkspace();

  // Callback ref instead of `useRef` because Radix Dialog mounts
  // `<DialogContent>` through a Portal — `useEffect` can fire one
  // tick BEFORE the portal commits, leaving `videoRef.current`
  // null and silently bailing the setup. With a state-backed ref,
  // React re-runs the effect once the <video> element attaches.
  const [videoEl, setVideoEl] = useState<HTMLVideoElement | null>(null);
  const [status, setStatus] = useState<Status>("loading");
  const [transport, setTransport] = useState<Transport>(() => (isSafari() ? "hls" : "webrtc"));

  // Reset the transport choice every time the dialog opens with a
  // new camera, so a successful HLS fallback on camera A doesn't
  // pre-empt the WebRTC attempt on camera B.
  useEffect(() => {
    if (open && camera) {
      setTransport(isSafari() ? "hls" : "webrtc");
      setStatus("loading");
    }
  }, [open, camera]);

  // WebRTC effect — only runs when we've chosen the WebRTC path.
  // Tearing down the RTCPeerConnection in the cleanup is essential;
  // a leaked pc keeps the box's coturn allocation alive until TTL.
  useEffect(() => {
    if (!open || !camera || !videoEl || transport !== "webrtc") return;

    const effectiveWorkspaceId = workspace?.id ?? LOCAL_WORKSPACE_ID;
    let pc: RTCPeerConnection | null = null;
    let firstFrameTimer: number | undefined;
    let aborted = false;

    const fallbackToHls = (reason: string) => {
      if (aborted) return;
      console.warn("[CameraPlayerDialog] WebRTC failed, falling back to HLS:", reason);
      aborted = true;
      if (pc) {
        pc.close();
        pc = null;
      }
      setTransport("hls");
      setStatus("loading");
    };

    (async () => {
      try {
        const session = await CameraService.requestWebrtcSession(effectiveWorkspaceId, camera.id);
        if (aborted) return;

        pc = new RTCPeerConnection({
          iceServers: session.ice_servers.map((s) => ({
            urls: s.urls,
            username: s.username,
            credential: s.credential
          }))
        });

        // Receive-only — operator just watches, no microphone uplink.
        pc.addTransceiver("video", { direction: "recvonly" });
        pc.addTransceiver("audio", { direction: "recvonly" });

        pc.ontrack = (e) => {
          if (aborted || !videoEl) return;
          videoEl.srcObject = e.streams[0];
        };

        // First successful media frame flips status. If it never
        // arrives within the timeout the fallback kicks in.
        const onLoadedData = () => {
          if (aborted) return;
          window.clearTimeout(firstFrameTimer);
          setStatus("playing");
          console.debug("[CameraPlayerDialog] webrtc media flowing");
        };
        videoEl.addEventListener("loadeddata", onLoadedData, { once: true });

        pc.oniceconnectionstatechange = () => {
          if (!pc) return;
          console.debug("[CameraPlayerDialog] ice state:", pc.iceConnectionState);
          if (pc.iceConnectionState === "failed" || pc.iceConnectionState === "disconnected") {
            fallbackToHls(`ice state ${pc.iceConnectionState}`);
          }
        };

        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);

        // WHEP: POST the SDP offer as text/plain `application/sdp`,
        // get the answer back as the response body. Direct browser
        // -> Funnel call, NOT through apiClient — apiClient would
        // inject the Oxy bearer which the WHEP endpoint doesn't
        // know about and would reject.
        const res = await fetch(session.whep_url, {
          method: "POST",
          headers: { "Content-Type": "application/sdp" },
          body: offer.sdp
        });
        if (!res.ok) {
          fallbackToHls(`WHEP POST returned ${res.status}`);
          return;
        }
        const answerSdp = await res.text();
        if (aborted) return;

        await pc.setRemoteDescription({ type: "answer", sdp: answerSdp });

        // Start the timeout AFTER signaling completes — ICE +
        // DTLS happens in the background, and the loadeddata event
        // is what we actually wait for.
        firstFrameTimer = window.setTimeout(() => {
          if (videoEl?.readyState === 0) {
            fallbackToHls("no media within timeout");
          }
        }, WEBRTC_FIRST_FRAME_TIMEOUT_MS);
      } catch (err) {
        fallbackToHls(err instanceof Error ? err.message : "unknown error");
      }
    })();

    return () => {
      aborted = true;
      window.clearTimeout(firstFrameTimer);
      if (pc) pc.close();
      if (videoEl) videoEl.srcObject = null;
    };
  }, [open, camera, workspace?.id, videoEl, transport]);

  // HLS effect — original path, runs when transport === "hls".
  // The dialog lands here either from a Safari direct hit or from
  // the WebRTC effect's fallback. Identical behavior in both
  // cases — the user just sees a slightly slower stream.
  useEffect(() => {
    if (!open || !camera || !videoEl || transport !== "hls") return;

    const effectiveWorkspaceId = workspace?.id ?? LOCAL_WORKSPACE_ID;
    const url = `${apiBaseURL}/${effectiveWorkspaceId}/cameras/${camera.id}/preview/hls/index.m3u8`;
    const authToken = localStorage.getItem("auth_token");

    if (!Hls.isSupported()) {
      console.warn("[CameraPlayerDialog] Hls.isSupported() === false");
      setStatus("unavailable");
      return;
    }

    const hls = new Hls({
      // Each fetch — playlist + every segment — needs the auth
      // token. hls.js routes XHR through this callback so we can
      // set headers (unlike a native <video src>).
      xhrSetup: (xhr) => {
        if (authToken) xhr.setRequestHeader("Authorization", authToken);
      }
    });

    const onManifestParsed = () => {
      console.debug("[CameraPlayerDialog] hls manifest parsed");
      setStatus("playing");
    };
    const onError = (
      _event: unknown,
      data: { fatal?: boolean; type?: string; details?: string }
    ) => {
      if (!data.fatal) return;
      console.error("[CameraPlayerDialog] fatal hls error", data);
      setStatus("unavailable");
    };

    hls.on(Hls.Events.MANIFEST_PARSED, onManifestParsed);
    hls.on(Hls.Events.ERROR, onError);
    hls.attachMedia(videoEl);
    hls.loadSource(url);

    return () => {
      hls.off(Hls.Events.MANIFEST_PARSED, onManifestParsed);
      hls.off(Hls.Events.ERROR, onError);
      hls.destroy();
    };
  }, [open, camera, workspace?.id, videoEl, transport]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-3xl'>
        <DialogHeader>
          <DialogTitle className='flex items-center gap-2'>
            {camera?.name ?? "Camera"}
            {status === "playing" && (
              <Badge
                variant={transport === "webrtc" ? "default" : "secondary"}
                className='text-[10px]'
              >
                {transport === "webrtc" ? (
                  <>
                    <Bolt className='size-3' /> live · low latency
                  </>
                ) : (
                  <>live · HLS</>
                )}
              </Badge>
            )}
          </DialogTitle>
          <DialogDescription>
            {camera?.site_name}
            {camera?.edge_box_name && ` · ${camera.edge_box_name}`}
          </DialogDescription>
        </DialogHeader>

        <div className='relative flex aspect-video w-full items-center justify-center overflow-hidden rounded-md border bg-muted/40'>
          <video
            ref={setVideoEl}
            autoPlay
            muted
            playsInline
            className='h-full w-full bg-black object-contain'
            controls={status === "playing"}
          />

          {status === "loading" && (
            <div className='absolute inset-0 flex items-center justify-center bg-background/70 backdrop-blur-sm'>
              <div className='flex flex-col items-center gap-2'>
                <Spinner />
                <p className='text-muted-foreground text-xs'>
                  Connecting via {transport === "webrtc" ? "WebRTC" : "HLS"}…
                </p>
              </div>
            </div>
          )}

          {status === "unavailable" && (
            <div className='absolute inset-0 flex items-center justify-center bg-background/80 backdrop-blur-sm'>
              <div className='flex max-w-md flex-col items-center gap-2 text-center'>
                <VideoOff className='size-6 text-muted-foreground' />
                <p className='font-medium'>Live preview not available</p>
                <p className='text-muted-foreground text-xs'>
                  This box hasn't reported a Tailscale Funnel hostname yet, so the browser can't
                  reach its MediaMTX stream. Common in dev/OrbStack VMs without Funnel enabled — the
                  rest of the pipeline (events, compliance reports, recordings) still works.
                </p>
                <p className='text-[10px] text-muted-foreground'>
                  Set <code>EDGE_FUNNEL_HOSTNAME</code> on the box's <code>.env</code> and restart
                  to enable live preview.
                </p>
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default CameraPlayerDialog;
