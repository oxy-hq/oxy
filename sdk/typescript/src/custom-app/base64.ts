// Base64 for binary that has to cross a boundary as text — an email
// attachment's `content`, a `ctx.storage.put` body.
//
// These are plain functions, deliberately NOT `btoa`/`atob`:
//
//   - `btoa` takes a Latin1 STRING. Handing it a `Uint8Array` is the classic
//     footgun: the spec stringifies it, so a PDF starting `%PDF` silently
//     encodes the text "37,80,68,70" and you ship a corrupt file. A named
//     function that takes bytes cannot be misused that way.
//   - Being ordinary bundled JS, they behave identically in the Oxy Functions
//     isolate, in Node/vitest, and in a browser. Anything reached through a
//     global risks being a different implementation in each.

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** Reverse lookup; 255 marks "not a base64 character". */
const B64R = /* @__PURE__ */ (() => {
  const t = new Uint8Array(256).fill(255);
  for (let i = 0; i < 64; i++) t[B64.charCodeAt(i)] = i;
  return t;
})();

/**
 * Chunk size for building output in segments. Byte-at-a-time `+=` allocates a
 * rope node per byte, and `String.fromCharCode.apply` blows the argument limit
 * on large inputs; 8k avoids both.
 */
const CHUNK = 8192;

function asBytes(input: Uint8Array | ArrayBuffer | ArrayBufferView): Uint8Array {
  if (input instanceof Uint8Array) return input;
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
}

/**
 * Encode bytes as standard (padded) base64.
 *
 * ```ts
 * const pdf = new Uint8Array(await renderReport());
 * await ctx.email.send({
 *   to: ctx.user.email,
 *   subject: "Report",
 *   text: "attached",
 *   attachments: [{ filename: "report.pdf", content: bytesToBase64(pdf) }]
 * });
 * ```
 *
 * For **text** you generated, skip this entirely and pass the string with
 * `encoding: "utf8"` — it needs no encoder and stays byte-exact for non-ASCII.
 */
export function bytesToBase64(input: Uint8Array | ArrayBuffer | ArrayBufferView): string {
  const bytes = asBytes(input);
  const parts: string[] = [];
  let buf = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i];
    const b1 = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
    const n = (b0 << 16) | (b1 << 8) | b2;
    buf +=
      B64[(n >> 18) & 63] +
      B64[(n >> 12) & 63] +
      (i + 1 < bytes.length ? B64[(n >> 6) & 63] : "=") +
      (i + 2 < bytes.length ? B64[n & 63] : "=");
    if (buf.length >= CHUNK) {
      parts.push(buf);
      buf = "";
    }
  }
  parts.push(buf);
  return parts.join("");
}

/**
 * Decode standard base64 to bytes — e.g. the body from
 * `ctx.storage.get(key, { encoding: "base64" })`.
 *
 * Throws on malformed input rather than returning a short buffer: a truncated
 * decode that reports success is a corrupt file nobody notices.
 */
export function base64ToBytes(base64: string): Uint8Array {
  let s = String(base64).replace(/[ \t\n\f\r]/g, "");
  // Strip padding first, and only at a multiple of 4 — matching WHATWG. A
  // decoder that stopped at the first "=" would silently truncate
  // `base64ToBytes(chunkA + chunkB)` when chunkA carries its own padding.
  if (s.length % 4 === 0) {
    let pad = 0;
    while (pad < 2 && s.charCodeAt(s.length - 1) === 61 /* = */) {
      s = s.slice(0, -1);
      pad++;
    }
  }
  if (s.indexOf("=") >= 0) {
    throw new TypeError("base64ToBytes: '=' may only appear as trailing padding");
  }
  if (s.length % 4 === 1) {
    throw new TypeError("base64ToBytes: invalid base64 length");
  }
  const out = new Uint8Array((s.length * 3) >> 2);
  let o = 0;
  let buf = 0;
  let bits = 0;
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    const v = code < 256 ? B64R[code] : 255;
    if (v === 255) {
      throw new TypeError(`base64ToBytes: invalid base64 character '${s[i]}'`);
    }
    buf = (buf << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[o++] = (buf >> bits) & 0xff;
    }
  }
  return out.subarray(0, o);
}
