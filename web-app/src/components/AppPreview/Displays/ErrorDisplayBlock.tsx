import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/shadcn/alert";
import { ErrorDisplay } from "@/types/app";
import { CircleAlert } from "lucide-react";

const ErrorDisplayBlock = ({ display }: { display: ErrorDisplay }) => (
  <Alert variant="destructive">
    <CircleAlert />
    <AlertTitle>{display.title}</AlertTitle>
    <AlertDescription>Error: {display.error}</AlertDescription>
  </Alert>
);

export default ErrorDisplayBlock;
