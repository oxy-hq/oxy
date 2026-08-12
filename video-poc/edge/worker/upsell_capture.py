"""Audio capture for upsell detection — ffmpeg → in-memory PCM chunks.

Pulls the employee-mic audio off the register camera's RTSP and yields raw
PCM to the caller. **Nothing is written to disk** — the bytes flow through
memory to the STT engine and are dropped (transcribe-and-discard). That's
the privacy posture: the register mic is tuned to the employee side and we
never retain audio.

Audio is pulled DIRECTLY from the camera RTSP, not from MediaMTX: the MTX
bridge drops audio (`-an`), so the republished path carries video only.
ffmpeg demuxes Protect's AAC fine (the AU-Index quirk only breaks MTX's
native depacketizer). Runtime port of `video-poc/audio-poc/capture.py`;
kept in lockstep with it — the edge image can't import that POC package.
"""
from __future__ import annotations

import os
import subprocess
import threading
import time
from collections.abc import Iterator

import numpy as np

SAMPLE_RATE = 16_000  # what whisper wants

# Speech-cleanup filter chain for faint / noisy far-field audio. Order
# matters: band-limit to the speech range (cut rumble + hiss), denoise the
# stationary background (afftdn), then AGC the level up so faint speech
# reaches a transcribable range (speechnorm). RECOVERS speech masked by
# noise or low level — it can't invent speech that wasn't captured.
ENHANCE_AF = os.environ.get(
    "UPSELL_ENHANCE_AF",
    "highpass=f=120,lowpass=f=4000,afftdn=nf=-25,speechnorm=e=12.5:r=0.0001:l=1",
)

# A half-open RTSP session (camera reboot, network blip, TCP stall) delivers no
# data, no EOF, and no error — so ffmpeg's read() blocks forever and the reader
# wedges (observed: windows frozen for a day). The watchdog in stream_pcm kills
# ffmpeg after this many seconds without a chunk so the AudioReader reconnects.
# Silence still produces PCM (zeros), so a data GAP this long is a genuine
# stall, not a quiet register.
STALL_TIMEOUT_SEC = float(os.environ.get("UPSELL_STALL_TIMEOUT_SEC", "20"))


def stream_pcm(
    source: str,
    *,
    sample_rate: int = SAMPLE_RATE,
    chunk_sec: float = 1.0,
    duration_sec: float | None = None,
    af: str | None = None,
    stop: threading.Event | None = None,
) -> Iterator[np.ndarray]:
    """Yield mono int16 PCM chunks (`chunk_sec` each) from `source` (a file
    path or `rtsp://…`). `af` is an ffmpeg audio-filter chain (e.g.
    ENHANCE_AF) applied before PCM output. `stop` lets a caller thread break
    the read loop and tear down ffmpeg promptly. Raises RuntimeError if
    ffmpeg exits non-zero with stderr."""
    cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error"]
    is_rtsp = source.startswith("rtsp")
    if is_rtsp:
        # Protect is TCP-only (UDP drops). Its audio DTS is also badly
        # discontinuous — left raw, real speech reaches Whisper as
        # hallucination. Regenerate timestamps from the wall clock and
        # resample-async (below) to smooth the gaps.
        cmd += ["-rtsp_transport", "tcp", "-use_wallclock_as_timestamps", "1"]
    cmd += ["-i", source, "-vn",              # drop video — audio only
            "-map", "0:a:0",                  # first audio track (the employee mic)
            "-ac", "1", "-ar", str(sample_rate)]
    filters = []
    if is_rtsp:
        filters.append("aresample=async=1:first_pts=0")  # repair the timeline
    if af:
        filters.append(af)                    # speech cleanup (denoise + AGC)
    if filters:
        cmd += ["-af", ",".join(filters)]
    if duration_sec:
        cmd += ["-t", str(duration_sec)]      # bound a live capture
    cmd += ["-f", "s16le", "-"]               # raw PCM to stdout
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    bytes_per_chunk = int(sample_rate * chunk_sec) * 2  # int16 = 2 bytes

    # Stall watchdog — kill ffmpeg if no chunk arrives for STALL_TIMEOUT_SEC
    # (killing it makes the blocking read() below return EOF), then flag it so
    # the loop raises and the AudioReader's reconnect loop takes over.
    last_data = [time.monotonic()]
    stalled = threading.Event()

    def _watchdog() -> None:
        while proc.poll() is None:
            if stop is not None and stop.is_set():
                proc.kill()
                return
            if time.monotonic() - last_data[0] > STALL_TIMEOUT_SEC:
                stalled.set()
                proc.kill()
                return
            time.sleep(1.0)

    threading.Thread(target=_watchdog, name="upsell-capture-watchdog", daemon=True).start()
    try:
        while True:
            if stop is not None and stop.is_set():
                return
            buf = proc.stdout.read(bytes_per_chunk)
            if not buf:
                if stalled.is_set():
                    raise RuntimeError(
                        f"audio stream stalled (no data for {STALL_TIMEOUT_SEC:.0f}s)"
                    )
                err = (proc.stderr.read() or b"").decode("utf-8", "replace")
                if proc.poll() not in (0, None) and err:
                    raise RuntimeError(f"ffmpeg: {err[-300:]}")
                return
            last_data[0] = time.monotonic()
            n = len(buf) & ~1  # a killed read can return an odd byte count
            if n:
                yield np.frombuffer(buf[:n], dtype=np.int16)
    finally:
        proc.kill()
        proc.wait(timeout=5)
