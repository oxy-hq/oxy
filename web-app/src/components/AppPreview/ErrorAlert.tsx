import { CircleAlert } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/shadcn/alert";
import { cn } from "@/libs/utils/cn";

interface ErrorAlertProps {
  children: React.ReactNode;
  className?: string;
}

const ErrorAlert = ({ children, className }: ErrorAlertProps) => (
  <Alert
    variant='destructive'
    className={cn(
      "border-none bg-destructive/10 p-2.5 font-mono text-destructive [&>svg]:translate-y-0",
      className
    )}
  >
    <CircleAlert />
    {children}
  </Alert>
);

const ErrorAlertMessage = ({ children }: { children: React.ReactNode }) => (
  <AlertDescription className='font-mono text-xs'>{children}</AlertDescription>
);

export { ErrorAlert, ErrorAlertMessage };
