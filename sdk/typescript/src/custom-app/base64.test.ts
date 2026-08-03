import { describe, expect, it } from "vitest";

import { base64ToBytes, bytesToBase64 } from "./base64";

describe("bytesToBase64", () => {
  it("encodes bytes, not their String() form", () => {
    // The bug this helper exists to make impossible: btoa(new Uint8Array(...))
    // stringifies to "37,80,68,70" and encodes that text instead of the file.
    const pdf = new Uint8Array([0x25, 0x50, 0x44, 0x46]); // "%PDF"
    expect(bytesToBase64(pdf)).toBe("JVBERg==");
    expect(bytesToBase64(pdf)).not.toBe(btoa(String(pdf)));
  });

  it("accepts ArrayBuffer and views", () => {
    const buf = new Uint8Array([1, 2, 3, 4, 5]).buffer;
    expect(bytesToBase64(buf)).toBe(bytesToBase64(new Uint8Array(buf)));
    // A view with a non-zero offset must encode only its own window.
    const view = new Uint8Array(buf, 1, 2);
    expect(base64ToBytes(bytesToBase64(view))).toEqual(new Uint8Array([2, 3]));
  });

  it("pads like the platform encoder across every length residue", () => {
    for (let n = 0; n < 40; n++) {
      const bytes = new Uint8Array(n);
      for (let i = 0; i < n; i++) bytes[i] = (i * 37) & 0xff;
      const expected = btoa(String.fromCharCode(...bytes));
      expect(bytesToBase64(bytes)).toBe(expected);
    }
  });

  it("round-trips across the chunking boundary", () => {
    // CHUNK is 8192; cross it to catch a segment-assembly bug.
    const n = 30_000;
    const bytes = new Uint8Array(n);
    for (let i = 0; i < n; i++) bytes[i] = i & 0xff;
    expect(base64ToBytes(bytesToBase64(bytes))).toEqual(bytes);
  });
});

describe("base64ToBytes", () => {
  it("decodes what the platform decoder decodes", () => {
    expect(base64ToBytes("aGVsbG8=")).toEqual(new Uint8Array([104, 101, 108, 108, 111]));
    expect(base64ToBytes("aGVs")).toEqual(new Uint8Array([104, 101, 108]));
  });

  it("ignores wrapping whitespace", () => {
    expect(base64ToBytes("aGVs\nbG8=")).toEqual(base64ToBytes("aGVsbG8="));
  });

  // The realistic corruption: reassembling chunks that each carry padding.
  // Stopping at the first "=" would return a SHORT buffer and report success.
  it("throws on concatenated base64 rather than silently truncating", () => {
    const joined = bytesToBase64(new Uint8Array([1])) + bytesToBase64(new Uint8Array([2]));
    expect(() => base64ToBytes(joined)).toThrow();
  });

  it.each(["ab=c", "YQ=", "====", "a", "="])("rejects malformed input %j", (input) => {
    expect(() => base64ToBytes(input)).toThrow();
    // ...and so does the platform, so behaviour matches wherever this runs.
    expect(() => atob(input)).toThrow();
  });

  it("rejects non-alphabet characters", () => {
    expect(() => base64ToBytes("aGVsbG8*")).toThrow(/invalid base64 character/);
  });
});
