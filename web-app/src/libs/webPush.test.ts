import { describe, expect, it } from "vitest";
import {
  browserDeviceName,
  type PushEnvironment,
  pushNeedsHomeScreen,
  urlBase64ToUint8Array
} from "./webPush";

const IPHONE =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const IPHONE_CHROME =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/129.0.6668.46 Mobile/15E148 Safari/604.1";
const IPHONE_16_3 =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 16_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.3 Mobile/15E148 Safari/604.1";
const IPAD =
  "Mozilla/5.0 (iPad; CPU OS 16_4_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const ANDROID =
  "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36";

const env = (over: Partial<PushEnvironment>): PushEnvironment => ({
  userAgent: IPHONE,
  platform: "iPhone",
  maxTouchPoints: 5,
  standalone: false,
  ...over
});

describe("pushNeedsHomeScreen", () => {
  it("is true for an iPhone or iPad in a browser tab", () => {
    expect(pushNeedsHomeScreen(env({}))).toBe(true);
    expect(pushNeedsHomeScreen(env({ userAgent: IPHONE_CHROME }))).toBe(true);
    expect(pushNeedsHomeScreen(env({ userAgent: IPAD, platform: "iPad" }))).toBe(true);
  });

  it("is true for an iPad that asks for the desktop site", () => {
    expect(pushNeedsHomeScreen(env({ userAgent: MAC, platform: "MacIntel" }))).toBe(true);
  });

  it("is false once Oxygen runs from the Home Screen", () => {
    expect(pushNeedsHomeScreen(env({ standalone: true }))).toBe(false);
  });

  it("is false on a Mac, on Android, and on an iOS without web push", () => {
    expect(
      pushNeedsHomeScreen(env({ userAgent: MAC, platform: "MacIntel", maxTouchPoints: 0 }))
    ).toBe(false);
    expect(pushNeedsHomeScreen(env({ userAgent: ANDROID, platform: "Linux armv8l" }))).toBe(false);
    expect(pushNeedsHomeScreen(env({ userAgent: IPHONE_16_3 }))).toBe(false);
  });
});

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

describe("browserDeviceName", () => {
  it("names the browser and the system it runs on", () => {
    expect(browserDeviceName(MAC)).toBe("Safari on macOS");
    expect(browserDeviceName(IPHONE)).toBe("Safari on iPhone");
    expect(browserDeviceName(IPHONE_CHROME)).toBe("Chrome on iPhone");
    expect(browserDeviceName(ANDROID)).toBe("Chrome on Android");
    expect(
      browserDeviceName(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36 Edg/129.0.0.0"
      )
    ).toBe("Edge on Windows");
    expect(
      browserDeviceName("Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0")
    ).toBe("Firefox on Linux");
  });

  it("says just 'Browser' for a user agent it cannot read", () => {
    expect(browserDeviceName("curl/8.0")).toBe("Browser");
  });
});
