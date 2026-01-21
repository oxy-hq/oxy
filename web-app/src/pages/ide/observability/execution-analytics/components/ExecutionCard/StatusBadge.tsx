import { CheckCircle, XCircle } from "lucide-react";

interface StatusBadgeProps {
  isSuccess: boolean;
}

export default function StatusBadge({ isSuccess }: StatusBadgeProps) {
  if (isSuccess) {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border bg-green-500/10 border-green-500/20 text-green-400">
        <CheckCircle className="h-3.5 w-3.5" />
        Success
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border bg-red-500/10 border-red-500/20 text-red-400">
      <XCircle className="h-3.5 w-3.5" />
      Failed
    </span>
  );
}
