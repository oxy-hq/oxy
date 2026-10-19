import { useCallback, useEffect, useReducer, useState } from "react";
import { useEnablePushOnThisDevice, useVapidPublicKey } from "@/hooks/api/notifications";
import {
  isThisDeviceSubscribed,
  pushNeedsHomeScreenHere,
  pushPermission,
  pushSupported
} from "@/libs/webPush";
import { isDismissed, PUSH_PROMPT_CHANGED, pushPromptState, rememberDismissed } from "./utils";

const announce = () => window.dispatchEvent(new Event(PUSH_PROMPT_CHANGED));

/**
 * Whether to offer push on this device, and the two things the row can do.
 * Reads the browser on every render (permission, support) and asks the worker
 * whether a subscription exists whenever push becomes askable. Nothing here
 * asks for permission: only `turnOn`, from a click, does.
 */
export function usePushPrompt() {
  const vapid = useVapidPublicKey();
  const enable = useEnablePushOnThisDevice();
  const publicKey = vapid.data?.configured ? vapid.data.public_key : null;

  const [dismissed, setDismissed] = useState(() => isDismissed(Date.now()));
  const [subscribed, setSubscribed] = useState<boolean | undefined>(undefined);
  // Permission and support are read from the browser during render, so when
  // another prompt changes them (push turned on there) this one only needs to
  // render again. A subscription cannot appear while permission is still
  // "default", so the check below needs no re-run for it: `askable` is its key.
  const [, rerender] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    const onChange = () => {
      // Never un-dismiss: where storage is blocked, this prompt's own "×" is
      // remembered only here, and its own announcement must not undo it.
      setDismissed((was) => was || isDismissed(Date.now()));
      rerender();
    };
    window.addEventListener(PUSH_PROMPT_CHANGED, onChange);
    return () => window.removeEventListener(PUSH_PROMPT_CHANGED, onChange);
  }, []);

  const supported = pushSupported();
  const permission = pushPermission();
  const askable = publicKey !== null && supported && permission === "default";

  useEffect(() => {
    if (!askable) return;
    let live = true;
    isThisDeviceSubscribed().then(
      (has) => {
        if (live) setSubscribed(has);
      },
      (err: unknown) => {
        // Unknown is not "no": better no prompt than one for a device that has push.
        console.warn("Couldn't read this device's push subscription", err);
      }
    );
    return () => {
      live = false;
    };
  }, [askable]);

  useEffect(() => {
    if (enable.isSuccess) announce();
  }, [enable.isSuccess]);

  const state = pushPromptState({
    configured: publicKey !== null,
    supported,
    needsHomeScreen: pushNeedsHomeScreenHere(),
    permission,
    subscribed,
    dismissed: dismissed || enable.isSuccess
  });

  const { turnOn: enableWith } = enable;
  const turnOn = useCallback(() => {
    if (publicKey) enableWith(publicKey);
  }, [publicKey, enableWith]);

  const dismiss = useCallback(() => {
    rememberDismissed(Date.now());
    setDismissed(true);
    announce();
  }, []);

  return { state, turnOn, dismiss, pending: enable.isPending };
}
