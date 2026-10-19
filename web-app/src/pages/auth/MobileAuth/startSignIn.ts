import { initiateGoogleAuth } from "@/hooks/auth/useGoogleAuth";
import { initiateOktaAuth } from "@/hooks/auth/useOktaAuth";
import type { AuthConfigResponse } from "@/types/auth";
import { loginUrl, type SignInProvider } from "./mobileAuthRequest";
import { leaveTo } from "./mobileSession";

/**
 * Starts the provider's own sign-in, coming back to `returnTo`, and answers whether it did. It
 * doesn't for a provider this deployment doesn't offer, or one that can't start without a tap.
 */
const startProvider = async (
  provider: SignInProvider | null,
  config: AuthConfigResponse,
  returnTo: string
): Promise<boolean> => {
  switch (provider) {
    case "google":
      if (!config.google) return false;
      await initiateGoogleAuth(config.google.client_id, returnTo);
      return true;
    case "okta":
      if (!config.okta) return false;
      await initiateOktaAuth(config.okta.client_id, config.okta.domain, returnTo);
      return true;
    case "email":
      if (!config.magic_link) return false;
      leaveTo(loginUrl(returnTo, "email"));
      return true;
    case "github":
      // GitHub signs in through a popup, and a browser blocks a popup no tap opened: the login
      // page's GitHub button is that tap.
      return false;
    case null:
      return false;
  }
};

/**
 * Sends a signed-out browser to sign in: straight to the provider the app named when it can,
 * otherwise (none named, not offered here, or its start failed) to the login page, as before.
 */
export const startSignIn = async (
  provider: SignInProvider | null,
  config: AuthConfigResponse,
  returnTo: string
): Promise<void> => {
  const started = await startProvider(provider, config, returnTo).catch((error: unknown) => {
    console.error(`Failed to start ${provider} sign-in:`, error);
    return false;
  });
  if (!started) leaveTo(loginUrl(returnTo));
};
