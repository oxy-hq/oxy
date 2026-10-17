// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { AxiosError, type AxiosResponse } from "axios";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { settingsDoc } from "@/components/settings/SettingsDialog/sections/account/Notifications/__tests__/fixtures";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationSettings, NotificationSettingsPatch } from "@/types/notifications";
import queryKeys from "../queryKey";

vi.mock("@/services/api/notifications", () => ({
  NotificationsService: { updateSettings: vi.fn(), settings: vi.fn(), catalog: vi.fn() }
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { applySettingsPatch } from "./settingsPatch";
import { useUpdateNotificationSettings } from "./useUpdateNotificationSettings";

const service = vi.mocked(NotificationsService);
const KEY = queryKeys.notifications.settings();

const tempCheckPush = (doc: NotificationSettings | undefined) =>
  doc?.sources[0].categories[0].categories[0].kinds[0].settings.channels.push;
const MUTE_TEMP_CHECK: NotificationSettingsPatch = {
  nodes: [{ app_id: "app-1", kind: "temp_check", channels: { push: false } }]
};

/** A 412 carrying the server's current document. */
const stale = (current: NotificationSettings) =>
  new AxiosError("Precondition Failed", "ERR_BAD_REQUEST", undefined, undefined, {
    status: 412,
    data: current
  } as AxiosResponse);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (err: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function setup(doc: NotificationSettings = settingsDoc()) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }
  });
  qc.setQueryData(KEY, doc);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  const { result } = renderHook(() => useUpdateNotificationSettings(), { wrapper });
  return { qc, result, cached: () => qc.getQueryData<NotificationSettings>(KEY) };
}

beforeEach(() => {
  service.catalog.mockResolvedValue({ sources: [], reach: { push_devices: 0, email: true } });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("useUpdateNotificationSettings", () => {
  it("shows the change at once, sends it against the cached version, then takes the answer", async () => {
    const answer = deferred<NotificationSettings>();
    service.updateSettings.mockReturnValue(answer.promise);
    const { result, cached } = setup();

    act(() => result.current.mutate(MUTE_TEMP_CHECK));
    await waitFor(() => expect(tempCheckPush(cached())).toBe(false));
    expect(service.updateSettings).toHaveBeenCalledWith(MUTE_TEMP_CHECK, "v1");

    const saved = { ...applySettingsPatch(settingsDoc(), MUTE_TEMP_CHECK), version: "v2" };
    act(() => answer.resolve(saved));
    await waitFor(() => expect(cached()?.version).toBe("v2"));
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("undoes only this change when the server refuses it, and says so", async () => {
    service.updateSettings.mockRejectedValue(new Error("boom"));
    const { result, qc, cached } = setup();

    act(() => result.current.mutate(MUTE_TEMP_CHECK));
    // Something else changed the cache meanwhile; rolling back must not lose it.
    act(() => {
      qc.setQueryData<NotificationSettings>(KEY, (old) =>
        old ? { ...old, global: { ...old.global, timezone: "Europe/Berlin" } } : old
      );
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(tempCheckPush(cached())).toBeUndefined();
    expect(cached()?.global.timezone).toBe("Europe/Berlin");
    expect(toast.error).toHaveBeenCalledWith("Couldn't save that setting");
  });

  it("on a 412, applies the change again to the current document and sends it once more", async () => {
    // Someone else turned Oxygen's run emails off meanwhile: v5 on the server.
    const theirs = applySettingsPatch(settingsDoc({ version: "v5" }), {
      nodes: [{ app_id: null, scope: "runs", channels: { email: false } }]
    });
    const final = { ...applySettingsPatch(theirs, MUTE_TEMP_CHECK), version: "v6" };
    service.updateSettings.mockRejectedValueOnce(stale(theirs)).mockResolvedValueOnce(final);
    const { result, cached } = setup();

    act(() => result.current.mutate(MUTE_TEMP_CHECK));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(service.updateSettings).toHaveBeenNthCalledWith(1, MUTE_TEMP_CHECK, "v1");
    expect(service.updateSettings).toHaveBeenNthCalledWith(2, MUTE_TEMP_CHECK, "v5");
    expect(cached()).toEqual(final);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("on a 412 where the change is already true, takes that document and sends nothing more", async () => {
    const already = { ...applySettingsPatch(settingsDoc(), MUTE_TEMP_CHECK), version: "v5" };
    service.updateSettings.mockRejectedValueOnce(stale(already));
    const { result, cached } = setup();

    act(() => result.current.mutate(MUTE_TEMP_CHECK));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(service.updateSettings).toHaveBeenCalledTimes(1);
    expect(cached()).toEqual(already);
  });

  it("on a 412 where the change no longer applies, shows the current document and says so", async () => {
    // The app is gone from the server's document.
    const current = settingsDoc({ version: "v5" });
    current.sources = current.sources.filter((s) => s.app?.id !== "app-1");
    service.updateSettings.mockRejectedValueOnce(stale(current));
    const { result, cached } = setup();

    act(() => result.current.mutate(MUTE_TEMP_CHECK));
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(service.updateSettings).toHaveBeenCalledTimes(1);
    expect(cached()).toEqual(current);
    expect(toast.error).toHaveBeenCalledWith("Settings changed elsewhere, so that wasn't saved");
  });

  it("sends quick changes one at a time, each against the version the last one left", async () => {
    const first = deferred<NotificationSettings>();
    const second = deferred<NotificationSettings>();
    service.updateSettings.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const pause: NotificationSettingsPatch = { global: { paused_until: "2026-10-18T08:00:00Z" } };
    const { result, cached } = setup();

    act(() => {
      result.current.mutate(MUTE_TEMP_CHECK);
      result.current.mutate(pause);
    });
    // Both show at once; only the first has gone out.
    await waitFor(() => expect(cached()?.global.paused_until).toBe("2026-10-18T08:00:00Z"));
    expect(tempCheckPush(cached())).toBe(false);
    expect(service.updateSettings).toHaveBeenCalledTimes(1);

    act(() =>
      first.resolve({ ...applySettingsPatch(settingsDoc(), MUTE_TEMP_CHECK), version: "v2" })
    );
    await waitFor(() => expect(service.updateSettings).toHaveBeenCalledTimes(2));
    expect(service.updateSettings).toHaveBeenNthCalledWith(2, pause, "v2");
    // The first answer took only its version: the second change is still on screen.
    expect(cached()?.version).toBe("v2");
    expect(cached()?.global.paused_until).toBe("2026-10-18T08:00:00Z");

    const afterBoth = {
      ...applySettingsPatch(applySettingsPatch(settingsDoc(), MUTE_TEMP_CHECK), pause),
      version: "v3"
    };
    act(() => second.resolve(afterBoth));
    await waitFor(() => expect(cached()).toEqual(afterBoth));
  });
});
