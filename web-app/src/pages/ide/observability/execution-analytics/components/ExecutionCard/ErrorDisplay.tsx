import { AlertCircle } from "lucide-react";

interface ErrorDisplayProps {
  error: string;
}

export default function ErrorDisplay({ error }: ErrorDisplayProps) {
  return (
    <div className="flex items-start gap-2 p-3 rounded-lg border bg-red-500/10 border-red-500/20">
      <AlertCircle className="w-4 h-4 text-red-400 mt-0.5 flex-shrink-0" />
      <div>
        <p className="text-sm font-medium text-red-400">Execution Error</p>
        <code className="text-xs text-red-300 mt-1 block">{error}</code>
      </div>
    </div>
  );
}
