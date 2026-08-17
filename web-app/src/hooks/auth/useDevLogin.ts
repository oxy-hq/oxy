import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { AuthService } from "@/services/api";
import type { AuthResponse, DevLoginRequest } from "@/types/auth";
import { handlePostLoginOrgs, resolveReturnTo } from "./postLoginRedirect";

/**
 * A `?next=` destination is only followed when it is a same-origin path — one
 * leading slash, no protocol-relative `//host` — so the dev-login URL can't be
 * turned into an open redirect. Cross-origin destinations go through
 * `return_to`, which the server validates.
 */
export const sanitizeNextPath = (next: string | null | undefined): string | null => {
  if (!next?.startsWith("/") || next.startsWith("//")) {
    return null;
  }
  return next;
};

interface DevLoginOptions {
  /** Cross-origin post-login destination; validated server-side. */
  returnTo?: string;
  /** Same-origin path to land on, e.g. `/ide`. Wins over the org dispatcher. */
  next?: string | null;
  /**
   * Called when the server refuses. This has to be a **hook-level** callback,
   * not the `mutate(vars, { onError })` form: React Query skips the per-call
   * callbacks when the observer that issued the mutation is no longer
   * subscribed, and `/dev-login` fires from an effect whose StrictMode pass is
   * discarded immediately. Hook-level callbacks live on the mutation itself
   * and always run, so this is the only seam that reliably reports a refusal.
   */
  onFailure?: (error: Error) => void;
}

/**
 * Dev-only sign-in. Same post-login behavior as `useVerifyMagicLink` — store
 * the session, honor a validated `return_to`, otherwise land wherever the
 * user's orgs say — so a browser-automation run reaches exactly the state a
 * real sign-in would leave behind. `next` lets a test jump straight to the
 * page under test in a single navigation.
 */
export const useDevLogin = ({ returnTo, next, onFailure }: DevLoginOptions = {}) => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, DevLoginRequest>({
    mutationFn: AuthService.devLogin,
    onError: (error) => onFailure?.(error),
    onSuccess: async (data) => {
      login(data.token, data.user);

      const resolved = await resolveReturnTo(returnTo);
      if (resolved) {
        window.location.href = resolved;
        return;
      }

      const destination = sanitizeNextPath(next) ?? handlePostLoginOrgs(data.user, data.orgs);
      navigate(destination, { replace: true });
    }
  });
};
