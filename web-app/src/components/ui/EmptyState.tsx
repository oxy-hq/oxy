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
    <div
      className={cn(
        "flex items-center justify-center flex-col gap-4",
        className,
      )}
    >
      <img
        src={theme === "dark" ? "/oxy-light.svg" : "/oxy-dark.svg"}
        alt="No file"
        className="w-1/5 opacity-20"
      />
      <div className="text-center text-muted-foreground gap-1">
        <p className="text-lg">{title}</p>
        <p className="text-sm">{description}</p>
      </div>
    </div>
  );
};

export default EmptyState;
