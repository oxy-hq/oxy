"""On-device speech-to-text (faster-whisper), shared across audio readers.

`faster-whisper` is a CTranslate2 build of Whisper — CPU-friendly with int8,
which suits the edge box. The model is loaded once and shared across every
AudioReader; `transcribe()` is serialized by a lock (cheap — windows are
~12s apart and a box has at most a couple of register cameras).

`small.en` is the default. Against `base.en` on real far-field counter
audio, base.en collapsed into repeated-phrase hallucination loops ("Thank
you." ×25) even with `condition_on_previous_text=False`, while small.en
transcribed the same clip correctly; UPSELL_STT_MODEL overrides for a
CPU-bound box. Decode params mirror `video-poc/audio-poc/transcribe.py`.

The `faster_whisper` import is lazy (inside `__init__`) so the rest of the
worker imports without the STT dependency resolving — a box that never sets
UPSELL_CAMERAS never constructs a Transcriber.
"""
from __future__ import annotations

import os
import threading

import numpy as np

MODEL_SIZE = os.environ.get("UPSELL_STT_MODEL", "small.en")


class Transcriber:
    def __init__(
        self,
        model_size: str = MODEL_SIZE,
        *,
        device: str = "cpu",
        compute_type: str = "int8",
    ) -> None:
        from faster_whisper import WhisperModel  # lazy: keeps STT optional

        self._model = WhisperModel(model_size, device=device, compute_type=compute_type)
        self._lock = threading.Lock()

    def transcribe(self, pcm_int16: np.ndarray, sample_rate: int = 16_000) -> str:
        """Transcribe one PCM window → text. The array is the only copy of
        the audio and is dropped by the caller after this returns."""
        audio = pcm_int16.astype(np.float32) / 32768.0
        with self._lock:
            segments, _ = self._model.transcribe(
                audio,
                language="en",
                beam_size=1,
                temperature=0.0,
                # Kill the repeated-phrase hallucination on quiet/noisy
                # counter audio: don't feed the model its own previous
                # output, and let VAD + the no-speech gate drop near-silent
                # windows instead of "filling" them.
                condition_on_previous_text=False,
                vad_filter=True,
                vad_parameters={"min_silence_duration_ms": 500},
                # Faint far-field speech scores a high no_speech_prob (real
                # counter lines came in ~0.64), so keep the gate permissive —
                # VAD already strips true silence, which is where the looping
                # hallucination lived; this keeps real-but-quiet speech.
                no_speech_threshold=0.8,
            )
            return " ".join(seg.text.strip() for seg in segments).strip()
