/**
 * Whether oxyc routes through a proxy, and what it says when it can't.
 *
 * The module is faked: the real `setGlobalProxyFromEnv` changes the dispatcher
 * for the whole test process.
 */

import { describe, expect, it, vi } from "vitest";
import { proxyHint, readsEnvProxyVariable, routeThroughEnvProxy } from "./proxy.js";

const PROXY = { HTTPS_PROXY: "http://127.0.0.1:3128" };
const NODE_26 = { node: "26.7.0" };

describe("routeThroughEnvProxy", () => {
  it("does nothing without a proxy variable", () => {
    const set = vi.fn();
    expect(routeThroughEnvProxy({}, { setGlobalProxyFromEnv: set }, NODE_26)).toBe("none");
    expect(set).not.toHaveBeenCalled();
    expect(proxyHint()).toBeUndefined();
  });

  it("turns Node's proxy support on in-process where it can", () => {
    const set = vi.fn();
    expect(routeThroughEnvProxy(PROXY, { setGlobalProxyFromEnv: set }, NODE_26)).toBe("enabled");
    expect(set).toHaveBeenCalledWith(PROXY);
    expect(proxyHint()).toBeUndefined();
  });

  it("reads the lowercase variable too", () => {
    const env = { https_proxy: "http://127.0.0.1:3128" };
    expect(routeThroughEnvProxy(env, { setGlobalProxyFromEnv: vi.fn() }, NODE_26)).toBe("enabled");
  });

  it("leaves NODE_USE_ENV_PROXY=1 to Node, so a relaunched child never relaunches", () => {
    const set = vi.fn();
    const env = { ...PROXY, NODE_USE_ENV_PROXY: "1" };
    expect(routeThroughEnvProxy(env, {}, { node: "22.22.0" })).toBe("runtime");
    expect(routeThroughEnvProxy(env, { setGlobalProxyFromEnv: set }, NODE_26)).toBe("runtime");
    expect(set).not.toHaveBeenCalled();
  });

  it("leaves the proxy to Bun, which the standalone binaries run on", () => {
    expect(routeThroughEnvProxy(PROXY, {}, { node: "24.3.0", bun: "1.3.0" })).toBe("runtime");
  });

  /** A Claude cloud session: Node 22, a proxy, no in-process switch. */
  it("relaunches on a Node that only reads the variable at startup", () => {
    expect(routeThroughEnvProxy(PROXY, {}, { node: "22.21.0" })).toBe("relaunch");
    expect(routeThroughEnvProxy(PROXY, {}, { node: "24.10.0" })).toBe("relaunch");
    expect(proxyHint()).toBeUndefined();
  });

  it("says why on a Node that cannot proxy fetch at all", () => {
    expect(routeThroughEnvProxy(PROXY, {}, { node: "20.20.2" })).toBe("unsupported");
    expect(proxyHint()).toContain("Node 22.21+ or 24.5+");
  });

  /** The cloud-session hook exports the variable whatever Node is installed. */
  it("still says why when the variable is set on a Node that ignores it", () => {
    const env = { ...PROXY, NODE_USE_ENV_PROXY: "1" };
    expect(routeThroughEnvProxy(env, {}, { node: "22.20.0" })).toBe("unsupported");
    expect(proxyHint()).toContain("Node 22.21+ or 24.5+");
  });
});

describe("readsEnvProxyVariable", () => {
  it("knows the versions that added NODE_USE_ENV_PROXY", () => {
    expect(readsEnvProxyVariable("20.20.2")).toBe(false);
    expect(readsEnvProxyVariable("22.20.0")).toBe(false);
    expect(readsEnvProxyVariable("22.21.0")).toBe(true);
    expect(readsEnvProxyVariable("23.11.0")).toBe(false);
    expect(readsEnvProxyVariable("24.4.0")).toBe(false);
    expect(readsEnvProxyVariable("24.5.0")).toBe(true);
    expect(readsEnvProxyVariable("26.7.0")).toBe(true);
  });
});
