import OxyLogo from "@/components/OxyLogo";
import LoginForm from "./LoginForm";

export default function LoginPage() {
  return (
    <div className='customScrollbar grid h-full w-full overflow-auto bg-card'>
      <div className='flex flex-col gap-4 p-6 md:p-10'>
        <div className='flex justify-center gap-2 md:justify-start'>
          <a href='#' className='flex items-center gap-2 font-medium'>
            <OxyLogo />
            Oxy
          </a>
        </div>
        <div className='flex flex-1 items-center justify-center'>
          <div className='w-full max-w-xs'>
            <LoginForm />
          </div>
        </div>
      </div>
    </div>
  );
}
