import { ShieldCheck, Sparkles } from "lucide-react";

interface VerifiedBadgeProps {
  isVerified: boolean;
}

export default function VerifiedBadge({ isVerified }: VerifiedBadgeProps) {
  if (isVerified) {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border bg-emerald-500/10 border-emerald-500/20 text-emerald-400">
        <ShieldCheck className="h-3.5 w-3.5" />
        Verified
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border bg-orange-500/10 border-orange-500/20 text-orange-400">
      <Sparkles className="h-3.5 w-3.5" />
      Generated
    </span>
  );
}
