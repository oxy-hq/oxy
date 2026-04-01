import type { ErrorDisplay } from "@/types/app";
import { ErrorAlert, ErrorAlertMessage } from "../ErrorAlert";

const ErrorDisplayBlock = ({ display }: { display: ErrorDisplay }) => (
  <ErrorAlert>
    <ErrorAlertMessage>{display.title}</ErrorAlertMessage>
    <ErrorAlertMessage>Error: {display.error}</ErrorAlertMessage>
  </ErrorAlert>
);

export default ErrorDisplayBlock;
