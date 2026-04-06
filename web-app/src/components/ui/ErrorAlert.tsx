import { AlertCircle } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/libs/shadcn/utils";

interface ErrorAlertProps {
  title?: string;
  message?: ReactNode;
  icon?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  className?: string;
}

const ErrorAlert = ({ title, message, icon, actions, children, className }: ErrorAlertProps) => {
  const iconElement = icon ?? <AlertCircle className='mt-0.5 h-4 w-4 shrink-0 text-error' />;

  return (
    <div
      role='alert'
      className={cn(
        "flex items-start gap-2 rounded-md border border-error bg-error/10 p-3 text-left text-destructive",
        className
      )}
    >
      {iconElement}
      <div className='min-w-0 flex-1'>
        {title && <p className='text-error text-sm'>{title}</p>}
        {children ?? (
          <>
            {message && <div className={cn(title && "mt-0.5", "text-sm")}>{message}</div>}
            {actions && <div className='mt-2'>{actions}</div>}
          </>
        )}
      </div>
    </div>
  );
};

export default ErrorAlert;
