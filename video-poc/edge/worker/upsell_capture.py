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
    try:
        while True:
            if stop is not None and stop.is_set():
                return
            buf = proc.stdout.read(bytes_per_chunk)
            if not buf:
                err = (proc.stderr.read() or b"").decode("utf-8", "replace")
                if proc.poll() not in (0, None) and err:
                    raise RuntimeError(f"ffmpeg: {err[-300:]}")
                return
            yield np.frombuffer(buf, dtype=np.int16)
    finally:
        proc.kill()
        proc.wait(timeout=5)
