"""Upsell-intent classifier.

Given a short window of transcribed EMPLOYEE speech (the register mic is
tuned to the staff side), decide whether the employee made an *upsell
offer* — proactively offering a paid add-on / upgrade / extra ("would you
like to add avocado?", "want to make that a large?", "can I get you a
drink?") — as opposed to ordinary order-taking, confirming an order, or
merely mentioning a food item.

This is the feature's core novel risk: an item word alone ("avocado") is
not an upsell; the *offer* is. We use the same Haiku model the edge
already calls for the VLM, with a forced structured tool call so the
output is always a typed verdict.

No audio or transcript is persisted here — the caller passes text in and
gets a verdict out (transcribe-and-discard; see capture.py).
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass

import anthropic

# Same model family the edge uses for the VLM (worker/camera.py VLM_MODEL).
MODEL = os.environ.get("UPSELL_INTENT_MODEL", "claude-haiku-4-5-20251001")

_SYSTEM = (
    "You classify a transcript of a RESTAURANT COUNTER EMPLOYEE talking to a "
    "customer. The audio is the employee side only. Decide whether the "
    "EMPLOYEE made an UPSELL OFFER: proactively offering a paid add-on, extra, "
    "upgrade, size bump, drink, or side the customer had not already asked for "
    "(e.g. 'would you like to add avocado?', 'want to make it a large?', 'can I "
    "get you a drink with that?', 'add extra protein for $2?').\n\n"
    "It is NOT an upsell when the employee is: taking or repeating the "
    "customer's own order ('so that's a salmon bowl'), reading a total, "
    "greeting, giving directions, or merely MENTIONING an item without offering "
    "it ('we're out of avocado', 'the bowl comes with edamame'). A bare item "
    "name is never enough — there must be an offer to ADD or UPGRADE.\n\n"
    "Identify the specific item/upgrade offered when there is one."
)

_TOOL = {
    "name": "record_verdict",
    "description": "Record the upsell classification for this utterance.",
    "input_schema": {
        "type": "object",
        "properties": {
            "is_upsell": {
                "type": "boolean",
                "description": "True only if the employee proactively offered a paid add-on/upgrade.",
            },
            "item": {
                "type": ["string", "null"],
                "description": "The item or upgrade offered (e.g. 'avocado', 'large size', 'drink'), or null.",
            },
            "evidence": {
                "type": ["string", "null"],
                "description": "The exact phrase that constitutes the offer, or null.",
            },
            "confidence": {
                "type": "number",
                "description": "0.0-1.0 confidence that this is an upsell offer.",
            },
        },
        "required": ["is_upsell", "item", "evidence", "confidence"],
    },
}


@dataclass
class Verdict:
    is_upsell: bool
    item: str | None
    evidence: str | None
    confidence: float
    error: str | None = None


def classify(transcript: str, *, client: anthropic.Anthropic | None = None) -> Verdict:
    """Classify one transcript window. Never raises — on any API/parse error
    returns a non-upsell verdict flagged with `.error` so the pipeline keeps
    running (a missed detection beats a crashed stream)."""
    text = (transcript or "").strip()
    if not text:
        return Verdict(False, None, None, 0.0)
    client = client or anthropic.Anthropic()
    try:
        resp = client.messages.create(
            model=MODEL,
            max_tokens=300,
            system=_SYSTEM,
            tools=[_TOOL],
            tool_choice={"type": "tool", "name": "record_verdict"},
            messages=[{"role": "user", "content": f"Employee said: \"{text}\""}],
        )
        block = next(b for b in resp.content if getattr(b, "type", None) == "tool_use")
        v = block.input
        return Verdict(
            is_upsell=bool(v.get("is_upsell", False)),
            item=v.get("item") or None,
            evidence=v.get("evidence") or None,
            confidence=float(v.get("confidence", 0.0)),
        )
    except Exception as e:  # noqa: BLE001 — never break the stream
        return Verdict(False, None, None, 0.0, error=f"{type(e).__name__}: {e}")


def to_dict(v: Verdict) -> dict:
    return json.loads(json.dumps(v, default=lambda o: o.__dict__))
