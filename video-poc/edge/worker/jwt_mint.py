"""Device-side JWT minter.

Generates short-lived per-device JWTs the worker sends as
`Authorization: Bearer <jwt>` on every `/control/*` request.

Wire format mirrors the server-side verifier in
`crates/cameras/src/auth/jwt.rs`:

    base64url(header) "." base64url(payload) "." base64url(sig)

Header is the only thing we sign with the device secret. Payload
is `{iss, aud, exp}` — no `edge_box_id`, since the backend
resolves it via `device_claims` on every call. That keeps revoke
+ re-claim instant: the next minted JWT works against the new
claim without the device noticing.

The token is cached in-process until ~5 minutes before expiry so
we don't sign one per request (HMAC-SHA256 is cheap but not free,
and re-minting every call would obscure the production rate
limiter's "1 mint per box per hour" expectation). Returning the
cached token under the lock keeps `header()` safe to call from
the async drain loops AND from the sync `log` shipper.
"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import threading
import time
import uuid
from typing import Generator

import httpx

# Refresh ahead of expiry so an in-flight request never carries a
# JWT that expires mid-RTT. With a 1h TTL and a 5min refresh
# margin, the cache is good for ~55 minutes per signed token.
_TOKEN_TTL_SECS = 60 * 60  # 1 hour
_REFRESH_MARGIN_SECS = 5 * 60

# Audience constant — must match
# `oxy_cameras::auth::jwt::EXPECTED_AUD`. Drift here is silent
# until production where the operator sees "JWT rejected" with
# no further detail; keep the two strings in lockstep.
_EXPECTED_AUD = "oxy-control"


class JwtMinter:
    """Caches a JWT and re-mints on expiry.

    Thread-safe: a single lock guards the cached token + its
    expiry. The mint itself is fast enough (~µs) that we hold the
    lock through the HMAC computation rather than copying out
    + recomputing under a softer guarantee.
    """

    def __init__(self, device_id: uuid.UUID, device_secret: bytes) -> None:
        self._device_id = device_id
        self._device_secret = device_secret
        self._lock = threading.Lock()
        self._cached_token: str | None = None
        self._cached_exp: int = 0  # unix seconds; 0 = no cached token

    def header(self) -> str:
        """Return the Bearer header value (`Bearer <jwt>`)."""
        return f"Bearer {self.token()}"

    def token(self) -> str:
        """Return a valid JWT, minting a fresh one if the cache
        has expired or is within the refresh margin."""
        with self._lock:
            now = int(time.time())
            if self._cached_token and now + _REFRESH_MARGIN_SECS < self._cached_exp:
                return self._cached_token
            exp = now + _TOKEN_TTL_SECS
            self._cached_token = _sign(self._device_id, self._device_secret, exp)
            self._cached_exp = exp
            return self._cached_token

    def device_id(self) -> uuid.UUID:
        """Operator-facing accessor for `device_id` so logs can
        show which device a JWT was minted for without leaking
        the secret."""
        return self._device_id


def _sign(device_id: uuid.UUID, device_secret: bytes, exp: int) -> str:
    """Sign one JWT. Pure function — exposed for tests."""
    header_b64 = _b64url(b'{"alg":"HS256","typ":"JWT"}')
    payload_json = json.dumps(
        {"iss": str(device_id), "aud": _EXPECTED_AUD, "exp": exp},
        # Compact separators so the payload's bytes match what the
        # round-trip integration tests in tests/test_jwt_mint.py
        # expect; json.dumps' default whitespace would still be
        # spec-valid but adds bytes for nothing.
        separators=(",", ":"),
    ).encode()
    payload_b64 = _b64url(payload_json)
    signing_input = f"{header_b64}.{payload_b64}".encode()
    sig = hmac.new(device_secret, signing_input, hashlib.sha256).digest()
    sig_b64 = _b64url(sig)
    return f"{header_b64}.{payload_b64}.{sig_b64}"


def _b64url(data: bytes) -> str:
    """Base64url encoding with no padding — JWT spec."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


# ── httpx.Auth integrations ───────────────────────────────────────────────────


class DeviceJwtAuth(httpx.Auth):
    """httpx auth flow that stamps `Authorization: Bearer <jwt>` on
    every outbound request. Pulls the token from the supplied
    [`JwtMinter`] which caches under a lock — so the per-request
    overhead is one dict assignment, not a fresh HMAC.

    Wire this into both the main control client AND the outbox
    drain client (and any future per-loop httpx.AsyncClient). A
    single shared `JwtMinter` keeps the cached token consistent
    across them so we don't sign 5x as many JWTs as we need.
    """

    def __init__(self, minter: JwtMinter) -> None:
        self._minter = minter

    def auth_flow(self, request: httpx.Request) -> Generator[httpx.Request, httpx.Response, None]:
        request.headers["Authorization"] = self._minter.header()
        yield request


if __name__ == "__main__":
    # Manual sanity check: mint a token and dump its decoded
    # parts. Run `python -m worker.jwt_mint` on the box to verify
    # the chain locally before hitting the control plane.
    did = uuid.UUID("00000000-0000-7000-8000-000000000000")
    sec = bytes(range(32))
    m = JwtMinter(did, sec)
    tok = m.token()
    print(f"token: {tok}")
    h, p, _s = tok.split(".")
    print(f"header: {base64.urlsafe_b64decode(h + '==')!r}")
    print(f"payload: {base64.urlsafe_b64decode(p + '==')!r}")
