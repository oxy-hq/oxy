import { CheckCircle, XCircle } from "lucide-react";

interface StatusBadgeProps {
  isSuccess: boolean;
}

export default function StatusBadge({ isSuccess }: StatusBadgeProps) {
  if (isSuccess) {
    return (
      <span className='inline-flex items-center gap-1.5 rounded-full border border-green-500/20 bg-green-500/10 px-2.5 py-1 font-medium text-green-400 text-xs'>
        <CheckCircle className='h-3.5 w-3.5' />
        Success
      </span>
    );
  }

  return (
    <span className='inline-flex items-center gap-1.5 rounded-full border border-red-500/20 bg-red-500/10 px-2.5 py-1 font-medium text-red-400 text-xs'>
      <XCircle className='h-3.5 w-3.5' />
      Failed
    </span>
  );
}
