import { CircleAlert } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import type { ErrorDisplay } from "@/types/app";

const ErrorDisplayBlock = ({ display }: { display: ErrorDisplay }) => (
  <Alert variant='destructive'>
    <CircleAlert />
    <AlertTitle>{display.title}</AlertTitle>
    <AlertDescription>Error: {display.error}</AlertDescription>
  </Alert>
);

export default ErrorDisplayBlock;
