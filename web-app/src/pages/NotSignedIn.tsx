import LoginForm from "@/components/LoginForm";

// Authentication is handled at the reverse proxy layer using oauth2-proxy (see: https://oauth2-proxy.github.io/oauth2-proxy/configuration/providers/google).
// No cookie or auth check is necessary in client side code, as the reverse proxy will redirect to the NotSignedIn page if the user is not authenticated.

export default function NotSignedIn() {
  return (
    <div className="flex items-center justify-center h-screen w-screen bg-muted">
      <LoginForm />
    </div>
  );
}
