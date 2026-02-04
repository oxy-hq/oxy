import { ShieldCheck, Sparkles } from "lucide-react";

interface VerifiedBadgeProps {
  isVerified: boolean;
}

export default function VerifiedBadge({ isVerified }: VerifiedBadgeProps) {
  if (isVerified) {
    return (
      <span className='inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 font-medium text-emerald-400 text-xs'>
        <ShieldCheck className='h-3.5 w-3.5' />
        Verified
      </span>
    );
  }

  return (
    <span className='inline-flex items-center gap-1.5 rounded-full border border-orange-500/20 bg-orange-500/10 px-2.5 py-1 font-medium text-orange-400 text-xs'>
      <Sparkles className='h-3.5 w-3.5' />
      Generated
    </span>
  );
}
