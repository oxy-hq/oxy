import useTheme from "@/stores/useTheme";
import { LogIn } from "lucide-react";

// Authentication is handled at the reverse proxy layer using oauth2-proxy (see: https://oauth2-proxy.github.io/oauth2-proxy/configuration/providers/google).
// No cookie or auth check is necessary in client side code, as the reverse proxy will redirect to the NotSignedIn page if the user is not authenticated.
export default function LoginForm() {
  const { theme } = useTheme();
  return (
    <div className="flex flex-col items-center justify-center gap-6 bg-card rounded-2xl border border-border-foreground p-8">
      <img
        src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
        alt="Oxy"
        className="h-12 mb-2"
      />
      <a
        href={`${window.location.origin}/oauth2/sign_in`}
        className="flex items-center justify-center gap-2 w-full bg-primary text-primary-foreground rounded-md px-6 py-3 text-center font-medium hover:bg-primary/90 transition"
      >
        <LogIn className="w-5 h-5" />
        <span>Login with Google</span>
      </a>
    </div>
  );
}
