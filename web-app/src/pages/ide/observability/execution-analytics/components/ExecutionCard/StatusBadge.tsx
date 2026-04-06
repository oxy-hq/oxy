import { CheckCircle, XCircle } from "lucide-react";

interface StatusBadgeProps {
  isSuccess: boolean;
}

export default function StatusBadge({ isSuccess }: StatusBadgeProps) {
  if (isSuccess) {
    return (
      <span className='inline-flex items-center gap-1.5 rounded-full border border-success/20 bg-success/10 px-2.5 py-1 font-medium text-success text-xs'>
        <CheckCircle className='h-3.5 w-3.5' />
        Success
      </span>
    );
  }

  return (
    <span className='inline-flex items-center gap-1.5 rounded-full border border-destructive/20 bg-destructive/10 px-2.5 py-1 font-medium text-destructive text-xs'>
      <XCircle className='h-3.5 w-3.5' />
      Failed
    </span>
  );
}
