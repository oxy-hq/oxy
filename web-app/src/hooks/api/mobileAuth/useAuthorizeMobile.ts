import { useMutation } from "@tanstack/react-query";
import {
  type MobileAuthorizeRequest,
  type MobileAuthorizeResponse,
  MobileAuthService
} from "@/services/api/mobileAuth";

/**
 * Trade the browser session for a single-use code the Oxygen mobile app can exchange. No toast:
 * `/mobile-auth` is nothing but this request, so it renders the outcome itself.
 */
const useAuthorizeMobile = () =>
  useMutation<MobileAuthorizeResponse, Error, MobileAuthorizeRequest>({
    mutationFn: (request) => MobileAuthService.authorize(request)
  });

export default useAuthorizeMobile;
