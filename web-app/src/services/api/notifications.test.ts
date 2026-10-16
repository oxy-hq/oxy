import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ get: vi.fn(), post: vi.fn(), put: vi.fn() }));
vi.mock("./axios", () => ({ apiClient: api }));

import { NotificationsService } from "./notifications";

beforeEach(() => {
  api.get.mockReset().mockResolvedValue({ data: {} });
  api.post.mockReset().mockResolvedValue({ data: {} });
  api.put.mockReset().mockResolvedValue({ data: {} });
});

describe("NotificationsService.inbox", () => {
  it("sends only the bell's two params when nothing narrows it", async () => {
    await NotificationsService.inbox({ unreadOnly: false, limit: 50 });
    expect(api.get).toHaveBeenCalledWith("/notifications", {
      params: { unread_only: false, limit: 50 }
    });
  });

  it("sends the filter and the cursor; an app source is also app_id", async () => {
    await NotificationsService.inbox({
      unreadOnly: true,
      limit: 50,
      source: "app-1",
      category: "operations/food-safety",
      before: "n-9"
    });
    expect(api.get).toHaveBeenCalledWith("/notifications", {
      params: {
        unread_only: true,
        limit: 50,
        source: "app-1",
        app_id: "app-1",
        category: "operations/food-safety",
        before: "n-9"
      }
    });
  });

  it("narrows to Oxygen by source alone", async () => {
    await NotificationsService.inbox({
      unreadOnly: false,
      limit: 50,
      source: "oxygen",
      kind: "oxy.anomaly"
    });
    expect(api.get).toHaveBeenCalledWith("/notifications", {
      params: { unread_only: false, limit: 50, source: "oxygen", kind: "oxy.anomaly" }
    });
  });
});

describe("NotificationsService writes", () => {
  it("marks read what the filter covers, as query params", async () => {
    await NotificationsService.markAllRead({ source: "app-1", thread: "t-1" });
    expect(api.post).toHaveBeenCalledWith("/notifications/read-all", undefined, {
      params: { source: "app-1", app_id: "app-1", thread: "t-1" }
    });
  });

  it("marks everything read with no params", async () => {
    await NotificationsService.markAllRead();
    expect(api.post).toHaveBeenCalledWith("/notifications/read-all", undefined, { params: {} });
  });

  it("puts the scope form as given", async () => {
    await NotificationsService.setPreference({
      app_id: null,
      scope: "",
      channel: "email",
      muted: true
    });
    expect(api.put).toHaveBeenCalledWith("/notifications/preferences", {
      app_id: null,
      scope: "",
      channel: "email",
      muted: true
    });
  });

  it("runs an action by id, escaping both ids", async () => {
    api.post.mockResolvedValue({ data: { ok: true, url: "https://x/y" } });
    await expect(NotificationsService.runAction("n 1", "a/b")).resolves.toEqual({
      ok: true,
      url: "https://x/y"
    });
    expect(api.post).toHaveBeenCalledWith("/notifications/n%201/actions/a%2Fb");
  });
});

describe("NotificationsService reads", () => {
  it("reads the catalogue and one notification", async () => {
    await NotificationsService.catalog();
    await NotificationsService.item("n-1");
    expect(api.get).toHaveBeenNthCalledWith(1, "/notifications/catalog");
    expect(api.get).toHaveBeenNthCalledWith(2, "/notifications/n-1");
  });
});
