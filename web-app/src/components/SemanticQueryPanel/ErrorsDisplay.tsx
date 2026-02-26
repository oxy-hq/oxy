import { AlertCircle } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface ErrorBlockProps {
  title: string;
  message: string;
  variant?: "inline" | "card";
}

const ErrorBlock = ({ title, message, variant = "card" }: ErrorBlockProps) => {
  if (variant === "inline") {
    return (
      <div className='flex items-start gap-2 rounded-md border border-red-500/30 bg-red-950/30 p-2'>
        <AlertCircle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-red-400' />
        <div className='min-w-0 flex-1'>
          <span className='font-medium text-red-400 text-xs'>{title}: </span>
          <span className='whitespace-pre-wrap text-red-300 text-xs'>{message}</span>
        </div>
      </div>
    );
  }

  return (
    <div className='m-3 rounded-md border border-red-500/50 bg-red-900/20 p-3'>
      <div className='mb-1 flex items-center gap-1.5'>
        <AlertCircle className='h-3.5 w-3.5 text-red-400' />
        <h4 className='font-medium text-red-400 text-xs'>{title}</h4>
      </div>
      <pre className='whitespace-pre-wrap text-red-300 text-xs'>{message}</pre>
    </div>
  );
};

interface ErrorsDisplayProps {
  validationError?: string;
  sqlGenerationError?: string;
  executionError?: string;
  variant?: "inline" | "card";
  className?: string;
}

const ErrorsDisplay = ({
  validationError,
  sqlGenerationError,
  executionError,
  variant = "card",
  className
}: ErrorsDisplayProps) => {
  const hasErrors = validationError || sqlGenerationError || executionError;

  if (!hasErrors) return null;

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {validationError && (
        <ErrorBlock title='Validation Error' message={validationError} variant={variant} />
      )}
      {sqlGenerationError && (
        <ErrorBlock title='SQL Generation Error' message={sqlGenerationError} variant={variant} />
      )}
      {executionError && (
        <ErrorBlock title='Execution Error' message={executionError} variant={variant} />
      )}
    </div>
  );
};

export { ErrorBlock };
export default ErrorsDisplay;
