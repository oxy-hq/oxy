"""On-device speech-to-text (faster-whisper).

Runs on the edge box so audio never leaves the site. `faster-whisper` is a
CTranslate2 build of Whisper — CPU-friendly with int8, which suits the
t4g.medium edge box. VAD filtering trims silence so we don't burn cycles on
empty windows.

The import is lazy (inside `Transcriber`) so the rest of the package — the
Anthropic intent classifier and its eval — imports and runs without the STT
dependency installed.
"""
from __future__ import annotations

import os

import numpy as np

MODEL_SIZE = os.environ.get("UPSELL_STT_MODEL", "small.en")  # tiny/base.en for speed; small.en is more robust on far-field/noisy counter audio


class Transcriber:
    def __init__(self, model_size: str = MODEL_SIZE, *, device: str = "cpu",
                 compute_type: str = "int8") -> None:
        from faster_whisper import WhisperModel  # lazy: keeps STT optional

        self._model = WhisperModel(model_size, device=device, compute_type=compute_type)

    def transcribe(self, pcm_int16: np.ndarray, sample_rate: int = 16_000) -> str:
        """Transcribe one PCM window → text. The array is the only copy of the
        audio and is dropped by the caller after this returns."""
        audio = pcm_int16.astype(np.float32) / 32768.0
        segments, _ = self._model.transcribe(
            audio, language="en", beam_size=1, temperature=0.0,
            # Kill the repeated-phrase hallucination on quiet/noisy counter
            # audio: don't feed the model its own previous output, and let VAD +
            # the no-speech gate drop near-silent windows instead of "filling" them.
            condition_on_previous_text=False,
            vad_filter=True,
            vad_parameters={"min_silence_duration_ms": 500},
            # Faint far-field speech scores a high no_speech_prob (real counter
            # lines came in ~0.64), so keep the gate permissive — VAD already
            # strips true silence, which is where the looping hallucination
            # lived; this keeps the real-but-quiet speech.
            no_speech_threshold=0.8,
        )
        return " ".join(seg.text.strip() for seg in segments).strip()
