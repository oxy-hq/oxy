import { cn } from "@/libs/shadcn/utils";
import useTheme from "@/stores/useTheme";

interface EmptyStateProps {
  title: string;
  description: string;
  className?: string;
}

const EmptyState = ({ title, description, className }: EmptyStateProps) => {
  const { theme } = useTheme();
  return (
    <div className={cn("flex flex-col items-center justify-center gap-1", className)}>
      <img
        src={theme === "dark" ? "/oxy-light.svg" : "/oxy-dark.svg"}
        alt='No file'
        className='max-h-1/2 w-full max-w-1/5 opacity-20'
      />
      <div className='gap-1 text-center text-muted-foreground'>
        <p className='text-lg'>{title}</p>
        <p className='text-sm'>{description}</p>
      </div>
    </div>
  );
};

export default EmptyState;
