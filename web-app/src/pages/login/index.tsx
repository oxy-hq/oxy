import LoginForm from "./LoginForm";
import OxyLogo from "@/components/OxyLogo";

export default function LoginPage() {
  return (
    <div className="bg-card grid h-full w-full overflow-auto customScrollbar">
      <div className="flex flex-col gap-4 p-6 md:p-10">
        <div className="flex justify-center gap-2 md:justify-start">
          <a href="#" className="flex items-center gap-2 font-medium">
            <OxyLogo />
            Oxy
          </a>
        </div>
        <div className="flex flex-1 items-center justify-center">
          <div className="w-full max-w-xs">
            <LoginForm />
          </div>
        </div>
      </div>
    </div>
  );
}
