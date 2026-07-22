#!/usr/bin/env python3
"""A fake OpenAI-compatible LLM gateway for testing Oxy's vendor routing.

Stands in for a third-party gateway (LangDock, Groq, OpenRouter, vLLM, …) so you
can verify which endpoint Oxy calls and whether the resolved API key reaches the
wire — without a real provider account. It logs every request, so the assertion
is what the gateway *received*, not what the agent eventually answered.

    python3 gateway.py                      # /responses 404s, like LangDock
    RESPONSES_MODE=ok python3 gateway.py    # /responses works, like real OpenAI
    EXPECTED_KEY=sk-x PORT=9000 python3 gateway.py

Env:
    EXPECTED_KEY    key that must arrive as `Authorization: Bearer …`
                    (default: sk-smoke-test)
    PORT            listen port (default: 8099)
    RESPONSES_MODE  `404` (default) or `ok` — whether /responses is implemented

See README.md for the full config.yml / .agentic.yml recipes.
"""

import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

EXPECTED_KEY = os.environ.get("EXPECTED_KEY", "sk-smoke-test")
PORT = int(os.environ.get("PORT", "8099"))
RESPONSES_MODE = os.environ.get("RESPONSES_MODE", "404").lower()


def redact(value):
    """Never log a key verbatim — length + last 4 is enough to tell keys apart."""
    if not value:
        return "<absent>"
    return f"<{len(value)} chars, ...{value[-4:] if len(value) > 4 else '?'}>"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # replaced by our own request log

    def _json(self, code, payload):
        body = json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _sse_chat_completion(self):
        """Minimal streaming Chat Completions reply.

        Oxy's OpenAiCompatProvider always sends `"stream": true` and parses SSE,
        so a plain JSON 200 here would hang or fail to decode.
        """
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.end_headers()

        def chunk(delta, finish=None):
            payload = {
                "id": "chatcmpl-fake",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "fake-model",
                "choices": [
                    {"index": 0, "delta": delta, "finish_reason": finish}
                ],
            }
            self.wfile.write(f"data: {json.dumps(payload)}\n\n".encode())
            self.wfile.flush()

        chunk({"role": "assistant"})
        chunk({"content": "pong from the fake gateway"})
        chunk({}, finish="stop")
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length") or 0))
        auth = self.headers.get("authorization", "")
        token = auth[7:] if auth.lower().startswith("bearer ") else ""
        path = self.path.rstrip("/")

        print(f"\n--> POST {self.path}")
        print(f"    Authorization: {redact(token)}")

        # ── Responses API (what `vendor: openai` targets) ───────────────────
        if path.endswith("/responses"):
            if RESPONSES_MODE != "ok":
                # Body mirrors LangDock's real 404 so the log matches what
                # customers paste into support threads.
                print("    <-- 404  (Responses API not implemented here)")
                self._json(404, {
                    "message": "Not found",
                    "docs": "https://docs.langdock.com/product/api",
                })
                return
            print("    <-- 200  (RESPONSES_MODE=ok)")
            self._json(200, {"id": "resp-fake", "object": "response",
                             "output": [], "status": "completed"})
            return

        # ── Chat Completions (what `vendor: openai_compat` targets) ─────────
        if not path.endswith("/chat/completions"):
            print("    <-- 404  (unknown path)")
            self._json(404, {"message": "Not found"})
            return

        if not token:
            # Verbatim OpenAI wording — this is the string customers report
            # when a key fails to resolve and an empty header goes out.
            print("    <-- 401  (no key on the wire)")
            self._json(401, {"error": {
                "message": "You didn't provide an API key. You need to provide "
                           "your API key in an Authorization header using Bearer "
                           "auth (i.e. Authorization: Bearer YOUR_KEY).",
                "type": "invalid_request_error",
            }})
            return

        if token != EXPECTED_KEY:
            print(f"    <-- 401  (wrong key; wanted {redact(EXPECTED_KEY)})")
            self._json(401, {"error": {
                "message": "Incorrect API key provided.",
                "type": "invalid_request_error",
            }})
            return

        print("    <-- 200 SSE  (correct endpoint + key)")
        self._sse_chat_completion()


def main():
    print(f"fake LLM gateway on http://127.0.0.1:{PORT}/v1")
    print(f"  expecting Bearer      {redact(EXPECTED_KEY)}")
    print(f"  /v1/responses         {'200' if RESPONSES_MODE == 'ok' else '404'}")
    print("  /v1/chat/completions  401 without key, 200 SSE with it")
    try:
        HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
    except KeyboardInterrupt:
        sys.exit(0)


if __name__ == "__main__":
    main()
