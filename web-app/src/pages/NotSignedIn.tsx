import LoginForm from "@/components/LoginForm";

// Authentication is handled at the reverse proxy layer using oauth2-proxy (see: https://oauth2-proxy.github.io/oauth2-proxy/configuration/providers/google).
// No cookie or auth check is necessary in client side code, as the reverse proxy will redirect to the NotSignedIn page if the user is not authenticated.

export default function NotSignedIn() {
  return (
    <div className="flex items-center justify-center h-screen bg-background">
      <main className="absolute inset-0 w-full rounded-xl my-2 mr-2 shadow-[0px_1px_3px_0px_rgba(0,0,0,0.10),0px_1px_2px_0px_rgba(0,0,0,0.06)] filter blur-md brightness-75 transition-all duration-300 bg-background z-0" />
      <div className="fixed inset-0 flex items-center justify-center z-10">
        <LoginForm />
      </div>
    </div>
  );
}
