import { AlertCircle } from "lucide-react";

interface ErrorDisplayProps {
  error: string;
}

export default function ErrorDisplay({ error }: ErrorDisplayProps) {
  return (
    <div className='flex items-start gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3'>
      <AlertCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-red-400' />
      <div>
        <p className='font-medium text-red-400 text-sm'>Execution Error</p>
        <code className='mt-1 block text-red-300 text-xs'>{error}</code>
      </div>
    </div>
  );
}
