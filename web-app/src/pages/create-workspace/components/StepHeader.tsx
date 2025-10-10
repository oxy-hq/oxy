import { cn } from "@/libs/utils/cn";

export type Step = {
  id: string;
  label: string;
};

interface StepHeaderProps {
  steps: Step[];
  currentStep: string;
  className?: string;
}

export default function StepHeader({
  steps,
  currentStep,
  className,
}: StepHeaderProps) {
  const currentIndex = steps.findIndex((step) => step.id === currentStep);

  return (
    <div
      className={cn("flex items-center justify-center w-full mb-8", className)}
    >
      <div className="flex w-full max-w-[100px] mx-auto space-x-1">
        {steps.map((step, index) => {
          const isActive = index === currentIndex;
          const isCompleted = index < currentIndex;

          return (
            <div
              key={step.id}
              className={cn(
                "h-1 flex-1 rounded-sm",
                isActive && "bg-primary",
                isCompleted && "bg-primary",
                !isActive && !isCompleted && "bg-muted",
              )}
            />
          );
        })}
      </div>
    </div>
  );
}
