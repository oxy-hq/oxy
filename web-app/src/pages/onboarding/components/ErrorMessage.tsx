interface ErrorMessageProps {
  message: string;
}

const ErrorMessage = ({ message }: ErrorMessageProps) => (
  <div className="mb-6 border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/10 p-4 rounded-lg">
    <p className="text-red-700 dark:text-red-400">{message}</p>
  </div>
);

export default ErrorMessage;
