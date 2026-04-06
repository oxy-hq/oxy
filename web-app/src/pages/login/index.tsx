import useTheme from "@/stores/useTheme";
import LoginForm from "./LoginForm";

export default function LoginPage() {
  const { theme } = useTheme();
  return (
    <div className='grid h-full w-full overflow-auto'>
      <div className='flex flex-col gap-4 p-6 md:p-10'>
        <div className='flex justify-center gap-2 md:justify-start'>
          <a href='#' className='flex items-center gap-2 font-medium'>
            <img src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"} alt='Oxy' />
            <span className='truncate font-medium text-sm'>Oxygen</span>
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
