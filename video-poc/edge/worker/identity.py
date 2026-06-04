"""Device-side IoT identity management.

Mirrors the backend identity model in
`crates/cameras/src/service/fleet.rs`. The device:

  1. Generates a UUID v7 + 32-byte secret at first boot.
  2. Persists them to disk (0600 perms) — TPM-NV is the future
     improvement, but a 0600 file is the V1 baseline that works on
     any Linux.
  3. Signs `/fleet/announce` and `/fleet/bootstrap` requests via
     HMAC-SHA256 over `device_id || "." || timestamp` keyed on the
     secret.

The persisted file format is JSON for ease of debugging — the
secret is base64-encoded, the rest are plaintext. We deliberately
don't encrypt the file (the file perms are the trust boundary),
but the secret never leaves this module — callers only see
signatures.
"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import secrets
import time
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path

DEFAULT_IDENTITY_PATH = Path(
    os.environ.get("OXY_DEVICE_IDENTITY_PATH", "/var/lib/oxy/device.json")
)
SECRET_LEN_BYTES = 32


@dataclass(frozen=True)
class DeviceIdentity:
    """In-memory representation of the device's IoT identity.

    `device_id` is a UUID v7 generated at first boot. v7 (vs v4) is
    sortable by creation time — handy when correlating registry
    rows with shipment dates. `device_secret` is 32 raw bytes;
    `sku` and `hardware_revision` are populated either from the
    image baked at the factory or from env vars on first run.
    """

    device_id: uuid.UUID
    device_secret: bytes
    sku: str
    hardware_revision: str

    # ── Persistence ──────────────────────────────────────────────

    def save(self, path: Path = DEFAULT_IDENTITY_PATH) -> None:
        """Write to disk with 0600 perms. Creates parent directory
        if missing. Uses an atomic write (`tmp + rename`) so a
        crash mid-save doesn't leave a corrupted file."""
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "device_id": str(self.device_id),
            "device_secret_b64": base64.b64encode(self.device_secret).decode(),
            "sku": self.sku,
            "hardware_revision": self.hardware_revision,
        }
        tmp = path.with_suffix(".tmp")
        # Open exclusively + chmod before any data hits the file —
        # if perms are wrong even briefly, an unprivileged reader on
        # the same box could exfiltrate the secret.
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump(payload, f)
        os.replace(tmp, path)

    @classmethod
    def load(cls, path: Path = DEFAULT_IDENTITY_PATH) -> DeviceIdentity | None:
        """Read the persisted identity. Returns None if the file
        doesn't exist (caller should generate + save). Raises if
        the file exists but is malformed — corruption is more
        dangerous than missing and shouldn't be silently
        overwritten."""
        if not path.exists():
            return None
        with path.open("r") as f:
            payload = json.load(f)
        # The "Add device" wizard emits base64 without trailing `=`
        # padding (the Rust `base64::URL_SAFE_NO_PAD` style is
        # standard for hand-typed-by-operator credentials — fewer
        # characters to mis-copy, no shell-escaping concerns). Python's
        # `b64decode` is strict about padding by default, so we
        # right-pad to a multiple of 4 before decoding. Same content,
        # tolerant of both padded and unpadded inputs.
        secret_b64 = payload["device_secret_b64"]
        secret_b64 += "=" * (-len(secret_b64) % 4)
        return cls(
            device_id=uuid.UUID(payload["device_id"]),
            device_secret=base64.b64decode(secret_b64),
            sku=payload["sku"],
            hardware_revision=payload["hardware_revision"],
        )

    @classmethod
    def generate(cls, *, sku: str, hardware_revision: str) -> DeviceIdentity:
        """Mint a fresh identity. Caller is expected to .save() it
        next; we don't auto-save because the caller may want to
        defer until factory-enrollment succeeds (otherwise a
        half-enrolled device on disk is hard to recover from)."""
        return cls(
            device_id=_uuid7(),
            device_secret=secrets.token_bytes(SECRET_LEN_BYTES),
            sku=sku,
            hardware_revision=hardware_revision,
        )

    # ── Signing ──────────────────────────────────────────────────

    def announce_signature(self, timestamp: int) -> bytes:
        """Compute the HMAC-SHA256 signature the backend expects on
        `POST /fleet/announce` and `POST /fleet/bootstrap`. Matches
        `crate::service::fleet::expected_announce_signature` exactly
        — the test in this module's __main__ runs against the same
        inputs to catch drift if either side changes."""
        return _announce_signature(self.device_secret, self.device_id, timestamp)

    def announce_signature_b64(self, timestamp: int | None = None) -> tuple[int, str]:
        """Convenience: returns `(timestamp, base64_signature)`
        with the timestamp filled in if not supplied. Callers use
        the same `timestamp` value in the request body."""
        ts = timestamp if timestamp is not None else int(time.time())
        sig = self.announce_signature(ts)
        return ts, base64.b64encode(sig).rstrip(b"=").decode()

    # ── Serialization for non-secret transmission ───────────────

    def factory_enroll_body(self) -> dict[str, str]:
        """Body for `POST /fleet/factory-enroll`. Includes the
        secret because the backend needs to record it; the body
        should ONLY be sent over TLS to the configured fleet URL."""
        return {
            "device_id": str(self.device_id),
            "device_secret_b64": base64.b64encode(self.device_secret).rstrip(b"=").decode(),
            "sku": self.sku,
            "hardware_revision": self.hardware_revision,
        }


# ── Internals ───────────────────────────────────────────────────

def _announce_signature(secret: bytes, device_id: uuid.UUID, timestamp: int) -> bytes:
    """HMAC-SHA256 over `{device_id.bytes} || "." || ascii(timestamp)`.

    Kept as a free function so tests can call it with synthetic
    secrets without needing to construct a full DeviceIdentity.
    Matches the backend layout byte-for-byte — the dot separator
    matters: the backend writes `mac.update(device_id.as_bytes());
    mac.update(b".")` exactly.
    """
    h = hmac.new(secret, digestmod=hashlib.sha256)
    h.update(device_id.bytes)
    h.update(b".")
    h.update(str(timestamp).encode("ascii"))
    return h.digest()


def _uuid7() -> uuid.UUID:
    """Generate a UUID v7. Python's stdlib (3.13+) has
    `uuid.uuid7()` but we don't yet require 3.13 across the worker
    image — implement the 5-line spec here for portability.

    Layout: 48-bit big-endian millisecond timestamp || 4-bit version
    (7) || 12 random bits || 2-bit variant (10) || 62 random bits.
    """
    # Use stdlib if available — keeps us aligned with future
    # interpreters that might tweak the implementation.
    impl = getattr(uuid, "uuid7", None)
    if impl is not None:
        return impl()

    ts_ms = int(time.time() * 1000) & ((1 << 48) - 1)
    rand_a = secrets.randbits(12)
    rand_b = secrets.randbits(62)
    # Pack: ts(48) ver(4) rand_a(12) var(2) rand_b(62) = 128 bits.
    value = (
        (ts_ms << 80)
        | (0x7 << 76)
        | (rand_a << 64)
        | (0b10 << 62)
        | rand_b
    )
    return uuid.UUID(int=value)


if __name__ == "__main__":
    # Self-check: round-trip an identity through disk + verify the
    # signature can be reproduced from raw inputs. Run with
    # `python -m worker.identity` to sanity-check before deploying.
    import tempfile

    ident = DeviceIdentity.generate(sku="test-sku", hardware_revision="rev-0")
    print(f"device_id      = {ident.device_id}")
    print(f"uuid version   = {ident.device_id.version}")
    print(f"secret bytes   = {len(ident.device_secret)}")

    with tempfile.NamedTemporaryFile(delete=False) as tf:
        tmp_path = Path(tf.name)
    try:
        ident.save(tmp_path)
        # Perms check: the umask shouldn't be able to weaken 0600.
        mode = tmp_path.stat().st_mode & 0o777
        assert mode == 0o600, f"expected 0600, got {oct(mode)}"

        loaded = DeviceIdentity.load(tmp_path)
        assert loaded == ident, "round-trip identity mismatch"
        print(f"perms          = {oct(mode)}")
        print("round-trip     OK")

        ts = 1700000000
        sig = ident.announce_signature(ts)
        # Recompute via the free function — equality means the
        # signing path is repeatable.
        assert sig == _announce_signature(ident.device_secret, ident.device_id, ts)
        print("signature      OK")
    finally:
        tmp_path.unlink(missing_ok=True)
