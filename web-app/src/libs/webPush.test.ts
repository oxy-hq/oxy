import { describe, expect, it } from "vitest";
import { urlBase64ToUint8Array } from "./webPush";

describe("urlBase64ToUint8Array", () => {
  it("decodes unpadded base64url, the alphabet VAPID keys come in", () => {
    // 0xfb 0xff 0xbf is "-_-_" in base64url and "+/+/" in standard base64.
    expect(Array.from(urlBase64ToUint8Array("-_-_"))).toEqual([0xfb, 0xff, 0xbf]);
    // Needs one byte of padding restored.
    expect(Array.from(urlBase64ToUint8Array("AQI"))).toEqual([1, 2]);
    // Needs two.
    expect(Array.from(urlBase64ToUint8Array("AQ"))).toEqual([1]);
  });

  it("decodes a full 65-byte uncompressed P-256 key", () => {
    const key = `B${"A".repeat(86)}`;
    const bytes = urlBase64ToUint8Array(key);
    expect(bytes).toHaveLength(65);
    expect(bytes[0]).toBe(0x04);
  });
});
